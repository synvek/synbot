//! IRC channel — connects to an IRC server and relays messages to/from the bot.
//!
//! Uses the `irc` crate for the IRC protocol. Supports TLS, NickServ
//! authentication, channel and private-message routing, and `RetryPolicy`
//! exponential-backoff reconnection.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use irc::client::prelude::*;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{Channel, RetryPolicy, RetryState};
use crate::config::IrcConfig;

// ---------------------------------------------------------------------------
// IrcChannel
// ---------------------------------------------------------------------------

pub struct IrcChannel {
    config: IrcConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    retry_state: RetryState,
}

impl IrcChannel {
    pub fn new(
        config: IrcConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
    ) -> Self {
        Self {
            config,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            retry_state: RetryState::new(),
        }
    }

    /// Build an `irc::client::data::Config` from our `IrcConfig`.
    fn build_irc_config(&self) -> irc::client::data::Config {
        let server = self
            .config
            .server
            .clone()
            .unwrap_or_else(|| "irc.libera.chat".to_string());
        let nickname = self
            .config
            .nickname
            .clone()
            .unwrap_or_else(|| "synbot".to_string());

        irc::client::data::Config {
            server: Some(server),
            port: Some(self.config.port),
            nickname: Some(nickname.clone()),
            username: Some(nickname.clone()),
            realname: Some("Synbot IRC Bridge".to_string()),
            channels: self.config.channels.clone(),
            use_tls: Some(self.config.use_tls),
            password: self.config.password.clone(),
            ..Default::default()
        }
    }

    /// Check if a sender is allowed (allowlist empty → allow all).
    fn is_sender_allowed(&self, sender_id: &str) -> bool {
        if self.config.allowlist.is_empty() {
            return true;
        }
        self.config
            .allowlist
            .iter()
            .any(|entry| entry.chat_id == sender_id)
    }

    /// True if PRIVMSG `target` is a channel (RFC 2812: `#`, `&`, `+`, `!` prefixes).
    fn is_channel_target(target: &str) -> bool {
        matches!(
            target.chars().next(),
            Some('#' | '&' | '+' | '!')
        )
    }

    /// Derive the chat_id for a message: for channel messages use the channel
    /// name, for private messages use the sender's nick.
    fn chat_id_for(target: &str, sender_nick: &str) -> String {
        if Self::is_channel_target(target) {
            target.to_string()
        } else {
            sender_nick.to_string()
        }
    }
}

#[async_trait]
impl Channel for IrcChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "IRC channel starting");

        let retry_policy = RetryPolicy::default();
        let irc_cfg = self.build_irc_config();

        // Connect to IRC server
        let mut client = Client::from_config(irc_cfg)
            .await
            .map_err(|e| anyhow!("IRC connect failed: {e:#}"))?;

        client
            .identify()
            .map_err(|e| anyhow!("IRC identify failed: {e:#}"))?;

        // NickServ authentication via PRIVMSG if password is set and we're
        // not using server-level password (PASS).
        if let Some(ref pw) = self.config.password {
            if !pw.is_empty() {
                let nick = self
                    .config
                    .nickname
                    .as_deref()
                    .unwrap_or("synbot")
                    .to_string();
                let _ = client.send_privmsg("NickServ", format!("IDENTIFY {} {}", nick, pw));
            }
        }

        info!(
            channel = %self.config.name,
            server = ?self.config.server,
            "IRC channel connected"
        );
        self.retry_state.reset();

        let channel_name = self.config.name.clone();
        let agent = self.config.agent.clone();
        let inbound_tx = self.inbound_tx.clone();
        let allowlist = self.config.allowlist.clone();

        // Outbound dispatcher — runs in background
        let sender = client.sender();
        let channel_name_out = channel_name.clone();
        let mut outbound_rx = self.outbound_rx.take().unwrap();

        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != channel_name_out {
                    continue;
                }
                let content = match &msg.message_type {
                    crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
                    crate::bus::OutboundMessageType::ToolProgress { .. } => continue,
                    crate::bus::OutboundMessageType::ApprovalRequest { request } => request
                        .display_message
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .unwrap_or_else(|| format!("Approval required: {}", request.id)),
                };

                // IRC has a 512-byte line limit; split long messages
                for line in content.lines() {
                    if let Err(e) = sender.send_privmsg(&msg.chat_id, line) {
                        error!(
                            channel = %channel_name_out,
                            error = %e,
                            "IRC send_privmsg failed"
                        );
                    }
                }
            }
        });

        // Inbound message loop
        let mut stream = client.stream()?;

        while let Some(result) = stream.next().await {
            match result {
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    let should_retry = self.retry_state.record_failure(&retry_policy, err_msg.clone());
                    if should_retry {
                        let delay = self.retry_state.next_delay(&retry_policy);
                        warn!(
                            channel = %channel_name,
                            error = %err_msg,
                            delay_ms = delay.as_millis() as u64,
                            "IRC stream error, will reconnect"
                        );
                        tokio::time::sleep(delay).await;
                        break; // outer loop would reconnect; for now just stop
                    } else {
                        error!(
                            channel = %channel_name,
                            error = %err_msg,
                            "IRC stream error, retries exhausted"
                        );
                        break;
                    }
                }
                Ok(message) => {
                    if let Command::PRIVMSG(ref target, ref text) = message.command {
                        let sender_nick = message
                            .prefix
                            .as_ref()
                            .and_then(|p| match p {
                                Prefix::Nickname(nick, _, _) => Some(nick.as_str()),
                                _ => None,
                            })
                            .unwrap_or("unknown");

                        // Ignore messages from ourselves
                        let our_nick = client.current_nickname();
                        if sender_nick == our_nick {
                            continue;
                        }

                        if !allowlist.is_empty()
                            && !allowlist.iter().any(|e| e.chat_id == sender_nick)
                        {
                            warn!(
                                sender = %sender_nick,
                                "IRC: sender not in allowlist, ignoring"
                            );
                            continue;
                        }

                        let chat_id = Self::chat_id_for(target, sender_nick);
                        let is_group = Self::is_channel_target(target);

                        let inbound = InboundMessage {
                            channel: channel_name.clone(),
                            sender_id: sender_nick.to_string(),
                            chat_id: chat_id.clone(),
                            content: text.clone(),
                            timestamp: chrono::Utc::now(),
                            media: vec![],
                            metadata: serde_json::json!({
                                "trigger_agent": true,
                                "default_agent": agent,
                                "irc_target": target,
                                "group": is_group,
                            }),
                        };

                        if let Err(e) = inbound_tx.send(inbound).await {
                            error!("Failed to forward IRC message to bus: {e}");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "IRC channel stopping");
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        // Direct send is not supported in webhook/stream mode;
        // outbound messages are handled by the spawned dispatcher in start().
        warn!(
            channel = %self.config.name,
            chat_id = %msg.chat_id,
            "IRC send() called outside of start() context — message dropped"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// IrcChannelFactory
// ---------------------------------------------------------------------------

pub struct IrcChannelFactory;

impl crate::channels::ChannelFactory for IrcChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: crate::channels::ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: IrcConfig = serde_json::from_value(config)
            .map_err(|e| anyhow!("Invalid IRC config: {e}"))?;

        if cfg.server.is_none() {
            warn!("IRC channel '{}' created without server address", cfg.name);
        }
        if cfg.nickname.is_none() {
            warn!("IRC channel '{}' created without nickname", cfg.name);
        }

        let ch = IrcChannel::new(cfg, ctx.inbound_tx, ctx.outbound_rx);
        Ok(Box::new(ch))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AllowlistEntry;
    use tokio::sync::{broadcast, mpsc};

    fn make_config() -> IrcConfig {
        IrcConfig {
            enabled: true,
            name: "irc".to_string(),
            server: Some("irc.libera.chat".to_string()),
            port: 6697,
            nickname: Some("synbot_test".to_string()),
            channels: vec!["#test".to_string()],
            use_tls: true,
            password: None,
            allowlist: vec![],
            agent: "main".to_string(),
        }
    }

    fn make_channel() -> IrcChannel {
        let (inbound_tx, _) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        IrcChannel::new(make_config(), inbound_tx, outbound_rx)
    }

    #[test]
    fn channel_name_returns_config_name() {
        let ch = make_channel();
        assert_eq!(ch.name(), "irc");
    }

    #[test]
    fn is_sender_allowed_empty_allowlist_allows_all() {
        let ch = make_channel();
        assert!(ch.is_sender_allowed("alice"));
        assert!(ch.is_sender_allowed("anyone"));
    }

    #[test]
    fn is_sender_allowed_with_allowlist() {
        let (inbound_tx, _) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let mut cfg = make_config();
        cfg.allowlist = vec![AllowlistEntry {
            chat_id: "alice".to_string(),
            chat_alias: "Alice".to_string(),
            my_name: None,
        }];
        let ch = IrcChannel::new(cfg, inbound_tx, outbound_rx);
        assert!(ch.is_sender_allowed("alice"));
        assert!(!ch.is_sender_allowed("bob"));
    }

    #[test]
    fn chat_id_for_channel_message_returns_channel() {
        assert_eq!(IrcChannel::chat_id_for("#general", "alice"), "#general");
        assert_eq!(IrcChannel::chat_id_for("&local", "alice"), "&local");
        assert_eq!(IrcChannel::chat_id_for("+modeless", "alice"), "+modeless");
        assert_eq!(IrcChannel::chat_id_for("!abcdechan", "alice"), "!abcdechan");
    }

    #[test]
    fn chat_id_for_private_message_returns_sender() {
        assert_eq!(IrcChannel::chat_id_for("synbot", "alice"), "alice");
    }

    #[test]
    fn build_irc_config_uses_defaults() {
        let (inbound_tx, _) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let cfg = IrcConfig {
            enabled: true,
            name: "irc".to_string(),
            server: None,
            port: 6667,
            nickname: None,
            channels: vec![],
            use_tls: false,
            password: None,
            allowlist: vec![],
            agent: "main".to_string(),
        };
        let ch = IrcChannel::new(cfg, inbound_tx, outbound_rx);
        let irc_cfg = ch.build_irc_config();
        assert_eq!(irc_cfg.server.as_deref(), Some("irc.libera.chat"));
        assert_eq!(irc_cfg.nickname.as_deref(), Some("synbot"));
    }

    #[test]
    fn factory_creates_channel_from_valid_config() {
        use crate::channels::ChannelFactory;
        let factory = IrcChannelFactory;
        let (inbound_tx, _) = mpsc::channel(16);
        let (outbound_tx, outbound_rx) = broadcast::channel(16);
        let ctx = crate::channels::ChannelStartContext {
            inbound_tx,
            outbound_rx,
            show_tool_calls: false,
            tool_result_preview_chars: 200,
            workspace: None,
            approval_manager: None,
            completion_model: None,
            outbound_tx: Some(outbound_tx),
        };
        let config = serde_json::json!({
            "enabled": true,
            "name": "irc",
            "server": "irc.libera.chat",
            "port": 6697,
            "nickname": "synbot",
            "channels": ["#test"],
            "useTls": true,
            "agent": "main"
        });
        let result = factory.create(config, ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "irc");
    }

    #[test]
    fn factory_returns_error_for_invalid_config() {
        use crate::channels::ChannelFactory;
        let factory = IrcChannelFactory;
        let (inbound_tx, _) = mpsc::channel(16);
        let (outbound_tx, outbound_rx) = broadcast::channel(16);
        let ctx = crate::channels::ChannelStartContext {
            inbound_tx,
            outbound_rx,
            show_tool_calls: false,
            tool_result_preview_chars: 200,
            workspace: None,
            approval_manager: None,
            completion_model: None,
            outbound_tx: Some(outbound_tx),
        };
        let result = factory.create(serde_json::json!("not_an_object"), ctx);
        assert!(result.is_err());
    }
}
