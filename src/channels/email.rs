//! Email channel — poll IMAP for unread emails from a configured sender, reply via SMTP, mark read.
//!
//! Processes only unread emails received after `start_time` and from `from_sender`, in chronological
//! order (oldest first). For each email: send content to agent, wait for reply, send reply email,
//! mark as read, then continue to the next.

use anyhow::{Context, Result};
use async_imap::Session;
use chrono::{DateTime, TimeZone, Utc};
use futures_util::StreamExt;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use mail_parser::MessageParser;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio_native_tls::TlsConnector;
use tokio_native_tls::TlsStream;
use tracing::{debug, error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use crate::channels::Channel;
use crate::config::EmailConfig;

/// Chat id format: "from_addr:uid" so we can reply and mark the right message read.
const CHAT_ID_SEP: char = ':';

/// IMAP stream: either TLS-wrapped or plain TCP (async_imap requires Debug on the stream type).
#[derive(Debug)]
enum ImapStreamKind {
    Tls(TlsStream<TcpStream>),
    Plain(TcpStream),
}

impl AsyncRead for ImapStreamKind {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ImapStreamKind::Tls(s) => Pin::new(s).poll_read(cx, buf),
            ImapStreamKind::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ImapStreamKind {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            ImapStreamKind::Tls(s) => Pin::new(s).poll_write(cx, buf),
            ImapStreamKind::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ImapStreamKind::Tls(s) => Pin::new(s).poll_flush(cx),
            ImapStreamKind::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ImapStreamKind::Tls(s) => Pin::new(s).poll_shutdown(cx),
            ImapStreamKind::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

impl Unpin for ImapStreamKind {}

pub struct EmailChannel {
    config: EmailConfig,
    show_tool_calls: bool,
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    /// chat_id -> (from_addr, uid, reply_tx). When we get the outbound reply we send email, mark read, then signal.
    pending: Arc<RwLock<HashMap<String, (String, u32, oneshot::Sender<()>)>>>,
}

impl EmailChannel {
    pub fn new(
        config: EmailConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        tool_result_preview_chars: usize,
    ) -> Self {
        Self {
            config,
            show_tool_calls,
            tool_result_preview_chars,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn chat_id(from_addr: &str, uid: u32) -> String {
        format!("{}{}{}", from_addr, CHAT_ID_SEP, uid)
    }

    fn parse_chat_id(chat_id: &str) -> Option<(String, u32)> {
        let pos = chat_id.find(CHAT_ID_SEP)?;
        let from = chat_id[..pos].to_string();
        let uid = chat_id[pos + 1..].parse::<u32>().ok()?;
        Some((from, uid))
    }

    /// Parse start_time: empty = no lower bound; "YYYY-MM-DD" or RFC3339.
    fn parse_start_time(s: &str) -> Option<DateTime<Utc>> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.with_timezone(&Utc));
        }
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
        }
        None
    }

    fn imap_port(cfg: &crate::config::EmailServerConfig) -> u16 {
        if cfg.port != 0 {
            cfg.port
        } else if cfg.use_tls {
            993
        } else {
            143
        }
    }

    fn smtp_port(cfg: &crate::config::EmailServerConfig) -> u16 {
        if cfg.port != 0 {
            cfg.port
        } else if cfg.use_tls {
            465
        } else {
            587
        }
    }

    async fn connect_imap(&self) -> Result<Session<ImapStreamKind>> {
        let cfg = &self.config.imap;
        let host = cfg.host.clone();
        let port = Self::imap_port(cfg);
        let addr = format!("{}:{}", host, port);
        let tcp = TcpStream::connect(&addr)
            .await
            .context("IMAP TCP connect")?;
        let stream = if cfg.use_tls {
            let connector = native_tls::TlsConnector::new()
                .context("IMAP TLS connector")?;
            let tls_connector = TlsConnector::from(connector);
            let tls = tls_connector
                .connect(&host, tcp)
                .await
                .context("IMAP TLS connect")?;
            ImapStreamKind::Tls(tls)
        } else {
            ImapStreamKind::Plain(tcp)
        };
        let mut client = async_imap::Client::new(stream);
        let _ = client
            .read_response()
            .await
            .context("IMAP read greeting")?
            .context("IMAP no greeting")?;
        let mut session = client
            .login(cfg.username.clone(), cfg.password.clone())
            .await
            .map_err(|e| anyhow::anyhow!("IMAP login failed: {}", e.0))?;
        session.select("INBOX").await.context("IMAP SELECT INBOX")?;
        Ok(session)
    }

    /// Format IMAP Address to "local@host".
    fn envelope_from_addr(addr: &imap_proto::types::Address) -> String {
        let mb = addr
            .mailbox
            .as_ref()
            .map(|c| String::from_utf8_lossy(c.as_ref()));
        let host = addr
            .host
            .as_ref()
            .map(|c| String::from_utf8_lossy(c.as_ref()));
        match (mb, host) {
            (Some(m), Some(h)) => format!("{}@{}", m, h).to_lowercase(),
            (Some(m), None) => m.to_lowercase(),
            (None, Some(h)) => format!("@{}", h).to_lowercase(),
            (None, None) => String::new(),
        }
    }

    /// Fetch unread emails from the configured sender, after start_time, sorted old to new.
    async fn fetch_unread(
        &self,
        session: &mut Session<ImapStreamKind>,
    ) -> Result<Vec<(u32, String, String)>> {
        let since = Self::parse_start_time(&self.config.start_time);
        if let Some(ref since_dt) = since {
            info!(
                channel = %self.config.name,
                start_time = %since_dt,
                "Email channel: only messages after start_time will be processed"
            );
        }
        let from_sender = self.config.from_sender.trim().to_lowercase();
        let from_sender = if from_sender.is_empty() {
            return Ok(vec![]);
        } else {
            from_sender
        };

        let uids = session.uid_search("UNSEEN").await.context("IMAP UID SEARCH UNSEEN")?;
        let uids: Vec<u32> = uids.into_iter().collect();
        info!(
            channel = %self.config.name,
            unread_count = uids.len(),
            from_sender = %from_sender,
            "Email channel: UNSEEN search result"
        );
        if uids.is_empty() {
            return Ok(vec![]);
        }

        let uid_set = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        let fetch = session
            .uid_fetch(&uid_set, "(FLAGS ENVELOPE INTERNALDATE)")
            .await
            .context("IMAP UID FETCH")?;
        let (messages, first_skip, fetch_count): (
            Vec<(u32, DateTime<Utc>, String)>,
            Option<(u32, String, chrono::DateTime<Utc>, &'static str)>,
            usize,
        ) = {
            let mut fetch_stream = std::pin::pin!(fetch);
            let mut list = Vec::new();
            let mut first_skip: Option<(u32, String, chrono::DateTime<Utc>, &'static str)> = None;
            let mut fetch_count = 0usize;
            while let Some(msg) = fetch_stream.next().await {
                let msg = msg.context("fetch item")?;
                fetch_count += 1;
                let uid = msg.uid.unwrap_or(0);
                let from_addr = msg
                    .envelope()
                    .and_then(|e| e.from.as_ref())
                    .and_then(|v| v.first())
                    .map(Self::envelope_from_addr)
                    .unwrap_or_default();
                let dt = msg
                    .internal_date()
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);
                if from_addr != from_sender {
                    if first_skip.is_none() {
                        first_skip = Some((uid, from_addr.clone(), dt, "sender_mismatch"));
                    }
                    continue;
                }
                if let Some(since_dt) = since {
                    if dt < since_dt {
                        if first_skip.is_none() {
                            first_skip = Some((uid, from_addr.clone(), dt, "before_start_time"));
                        }
                        continue;
                    }
                }
                list.push((uid, dt, from_addr));
            }
            (list, first_skip, fetch_count)
        };

        let mut sorted = messages;
        sorted.sort_by(|a, b| a.1.cmp(&b.1));
        if sorted.is_empty() {
            info!(
                channel = %self.config.name,
                unread_checked = uids.len(),
                fetch_items_received = fetch_count,
                "Email channel: all unread skipped by from_sender or start_time filter"
            );
            if fetch_count == 0 {
                warn!(
                    channel = %self.config.name,
                    "Email channel: uid_fetch returned 0 items (expected {}). Check IMAP server or try different fetch query.",
                    uids.len()
                );
            } else if let Some((uid, envelope_from, msg_date, reason)) = first_skip {
                info!(
                    channel = %self.config.name,
                    example_uid = uid,
                    envelope_from = %envelope_from,
                    config_from = %from_sender,
                    example_msg_date = %msg_date,
                    skip_reason = reason,
                    "Email channel: example of skipped message"
                );
            }
            return Ok(vec![]);
        }
        info!(
            channel = %self.config.name,
            to_process = sorted.len(),
            "Email channel: after from_sender/start_time filter"
        );

        let mut out = Vec::with_capacity(sorted.len());
        for (uid, _dt, from_addr) in sorted {
            let body = self.fetch_body(session, uid).await?;
            out.push((uid, from_addr, body));
        }
        Ok(out)
    }

    async fn fetch_body(
        &self,
        session: &mut Session<ImapStreamKind>,
        uid: u32,
    ) -> Result<String> {
        let uid_set = uid.to_string();
        let mut fetch = session
            .uid_fetch(&uid_set, "(BODY.PEEK[])")
            .await
            .context("IMAP UID FETCH body")?;
        let msg = fetch.next().await.transpose().context("fetch body")?;
        let msg = match msg {
            Some(m) => m,
            None => return Ok(String::new()),
        };
        let raw = msg.body().unwrap_or_default();
        let parsed = MessageParser::default().parse(raw);
        let body = parsed
            .and_then(|m| {
                m.body_text(0)
                    .map(|s| s.to_string())
                    .or_else(|| m.body_html(0).map(|s| s.to_string()))
            })
            .unwrap_or_else(|| String::from_utf8_lossy(raw).trim().to_string());
        Ok(body.trim().to_string())
    }

    async fn mark_read(
        &self,
        session: &mut Session<ImapStreamKind>,
        uid: u32,
    ) -> Result<()> {
        let uid_set = uid.to_string();
        let mut store = session
            .uid_store(&uid_set, "+FLAGS (\\Seen)")
            .await
            .context("IMAP UID STORE Seen")?;
        while store.next().await.is_some() {}
        Ok(())
    }

    async fn run_outbound_listener(
        channel_name: String,
        mut outbound_rx: broadcast::Receiver<OutboundMessage>,
        pending: Arc<RwLock<HashMap<String, (String, u32, oneshot::Sender<()>)>>>,
        config: EmailConfig,
        show_tool_calls: bool,
        tool_result_preview_chars: usize,
    ) {
        while let Ok(msg) = outbound_rx.recv().await {
            if msg.channel != channel_name {
                continue;
            }
            let (content, is_chat) = match &msg.message_type {
                OutboundMessageType::Chat { content, .. } => (content.clone(), true),
                OutboundMessageType::ToolProgress {
                    tool_name,
                    status,
                    result_preview,
                } if show_tool_calls => {
                    let preview = if result_preview.len() > tool_result_preview_chars {
                        format!("{}...", result_preview.chars().take(tool_result_preview_chars).collect::<String>())
                    } else {
                        result_preview.clone()
                    };
                    (format!("🔧 {} — {}\n{}", tool_name, status, preview), false)
                }
                _ => continue,
            };
            let chat_id = msg.chat_id.clone();
            if is_chat {
                let entry = pending.write().await.remove(&chat_id);
                if let Some((from_addr, _uid, reply_tx)) = entry {
                    if let Err(e) =
                        Self::send_reply_static(&config, &from_addr, "Reply", &content, None).await
                    {
                        error!(error = %e, "Email channel: send reply failed");
                    }
                    let _ = reply_tx.send(());
                }
            }
        }
    }

    async fn send_reply_static(
        config: &EmailConfig,
        to_addr: &str,
        subject: &str,
        body: &str,
        in_reply_to: Option<&str>,
    ) -> Result<()> {
        let cfg = &config.smtp;
        let port = if cfg.port != 0 { cfg.port } else if cfg.use_tls { 465 } else { 587 };
        let creds = Credentials::new(cfg.username.clone(), cfg.password.clone());
        let mailer: AsyncSmtpTransport<Tokio1Executor> = if cfg.use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?
                .port(port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)?
                .port(port)
                .credentials(creds)
                .build()
        };
        let from_mailbox = Mailbox::new(None, cfg.username.parse().context("parse from")?);
        let to_mailbox = Mailbox::new(None, to_addr.parse().context("parse to")?);
        let mut builder = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(format!("Re: {}", subject));
        if let Some(irt) = in_reply_to {
            builder = builder.header(lettre::message::header::InReplyTo::from(irt.to_string()));
        }
        let email = builder.body(body.to_string()).context("build email")?;
        mailer.send(email).await.context("SMTP send")?;
        Ok(())
    }

    pub async fn start(&mut self) -> Result<()> {
        let channel_name = self.config.name.clone();
        let mut outbound_rx = self.outbound_rx.take().expect("start once");
        let pending = Arc::clone(&self.pending);
        let config = self.config.clone();
        let show_tool_calls = self.show_tool_calls;
        let tool_result_preview_chars = self.tool_result_preview_chars;
        tokio::spawn(async move {
            Self::run_outbound_listener(channel_name, outbound_rx, pending, config, show_tool_calls, tool_result_preview_chars).await;
        });

        let poll_interval = std::time::Duration::from_secs(self.config.poll_interval_secs);
        let from_sender = self.config.from_sender.trim().to_lowercase();
        info!(
            channel = %self.config.name,
            from_sender = %from_sender,
            poll_secs = self.config.poll_interval_secs,
            "Email channel started"
        );

        loop {
            debug!(channel = %self.config.name, "Email channel poll cycle start");
            match self.poll_and_process().await {
                Ok(()) => {}
                Err(e) => {
                    warn!(error = %e, "Email channel poll error");
                }
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn poll_and_process(&self) -> Result<()> {
        let mut session = self.connect_imap().await?;
        debug!(channel = %self.config.name, "Email channel IMAP connected");
        let list = self.fetch_unread(&mut session).await?;
        if list.is_empty() {
            return Ok(());
        }
        info!(
            channel = %self.config.name,
            count = list.len(),
            "Email channel: processing messages"
        );
        for (uid, from_addr, body) in list {
            let chat_id = Self::chat_id(&from_addr, uid);
            let (tx, rx) = oneshot::channel();
            {
                let mut p = self.pending.write().await;
                p.insert(chat_id.clone(), (from_addr.clone(), uid, tx));
            }
            let content = if body.is_empty() { "(no body)" } else { body.as_str() };
            info!(
                channel = %self.config.name,
                uid,
                from = %from_addr,
                chat_id = %chat_id,
                "Email channel: sending to agent"
            );
            let _ = self
                .inbound_tx
                .send(InboundMessage {
                    channel: self.config.name.clone(),
                    sender_id: from_addr.clone(),
                    chat_id: chat_id.clone(),
                    content: content.to_string(),
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::json!({}),
                })
                .await;
            match tokio::time::timeout(std::time::Duration::from_secs(600), rx).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    debug!(chat_id = %chat_id, "Email reply channel closed");
                }
                Err(_) => {
                    warn!(chat_id = %chat_id, "Email reply timeout (10 min)");
                }
            }
            if let Err(e) = self.mark_read(&mut session, uid).await {
                warn!(uid, error = %e, "Mark email as read failed");
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Channel for EmailChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        let channel_name = self.config.name.clone();
        let outbound_rx = self.outbound_rx.take().expect("start once");
        let pending = Arc::clone(&self.pending);
        let config = self.config.clone();
        let show_tool_calls = self.show_tool_calls;
        let tool_result_preview_chars = self.tool_result_preview_chars;
        tokio::spawn(async move {
            Self::run_outbound_listener(channel_name, outbound_rx, pending, config, show_tool_calls, tool_result_preview_chars).await;
        });

        let poll_interval = std::time::Duration::from_secs(self.config.poll_interval_secs);
        let from_sender = self.config.from_sender.trim().to_lowercase();
        info!(
            channel = %self.config.name,
            from_sender = %from_sender,
            poll_secs = self.config.poll_interval_secs,
            "Email channel started"
        );

        loop {
            debug!(channel = %self.config.name, "Email channel poll cycle start");
            match self.poll_and_process().await {
                Ok(()) => {}
                Err(e) => {
                    warn!(error = %e, "Email channel poll error");
                }
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        if msg.channel != self.config.name {
            return Ok(());
        }
        let (content, _is_chat) = match &msg.message_type {
            OutboundMessageType::Chat { content, .. } => (content.clone(), true),
            OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            } if self.show_tool_calls => {
                let preview = if result_preview.len() > self.tool_result_preview_chars {
                    format!("{}...", result_preview.chars().take(self.tool_result_preview_chars).collect::<String>())
                } else {
                    result_preview.clone()
                };
                (format!("🔧 {} — {}\n{}", tool_name, status, preview), false)
            }
            _ => return Ok(()),
        };
        let entry = {
            let mut guard = self.pending.write().await;
            guard.remove(&msg.chat_id)
        };
        if let Some((from_addr, _uid, _)) = entry {
            if let Err(e) =
                Self::send_reply_static(&self.config, &from_addr, "Reply", &content, None).await
            {
                tracing::error!(error = %e, "Email channel: send failed");
                return Err(e.into());
            }
        }
        Ok(())
    }
}
