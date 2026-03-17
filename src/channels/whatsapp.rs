//! WhatsApp channel — WhatsApp Business Cloud API.
//!
//! Receives inbound messages via an actix-web webhook endpoint and sends
//! outbound messages via the WhatsApp Cloud API REST endpoint.
//! Integrates `RetryPolicy` / `RetryState` for resilient outbound delivery
//! with exponential backoff on transient errors.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{Channel, RetryPolicy, RetryState};
use crate::config::{AllowlistEntry, WhatsAppConfig};

const WHATSAPP_API_BASE: &str = "https://graph.facebook.com/v17.0";

// ---------------------------------------------------------------------------
// WhatsApp Cloud API webhook payload types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WhatsAppWebhookPayload {
    pub object: String,
    pub entry: Vec<WhatsAppEntry>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppEntry {
    pub id: String,
    pub changes: Vec<WhatsAppChange>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppChange {
    pub value: WhatsAppChangeValue,
    pub field: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppChangeValue {
    pub messaging_product: Option<String>,
    pub metadata: Option<WhatsAppMetadata>,
    pub contacts: Option<Vec<WhatsAppContact>>,
    pub messages: Option<Vec<WhatsAppMessage>>,
    pub statuses: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppMetadata {
    pub display_phone_number: String,
    pub phone_number_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppContact {
    pub profile: WhatsAppProfile,
    pub wa_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppProfile {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppMessage {
    pub from: String,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub text: Option<WhatsAppTextContent>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppTextContent {
    pub body: String,
}

// ---------------------------------------------------------------------------
// WhatsApp Cloud API send message request
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    messaging_product: String,
    recipient_type: String,
    to: String,
    #[serde(rename = "type")]
    message_type: String,
    text: SendTextContent,
}

#[derive(Debug, Serialize)]
struct SendTextContent {
    preview_url: bool,
    body: String,
}

// ---------------------------------------------------------------------------
// WhatsAppChannel
// ---------------------------------------------------------------------------

pub struct WhatsAppChannel {
    config: WhatsAppConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    retry_state: RetryState,
    http_client: reqwest::Client,
}

impl WhatsAppChannel {
    pub fn new(
        config: WhatsAppConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            retry_state: RetryState::new(),
            http_client,
        }
    }

    /// Send a text message to a WhatsApp phone number via the Cloud API.
    async fn send_whatsapp_message(&self, to: &str, text: &str) -> Result<()> {
        let access_token = self
            .config
            .access_token
            .as_deref()
            .ok_or_else(|| anyhow!("WhatsApp access_token not configured"))?;
        let phone_number_id = self
            .config
            .phone_number_id
            .as_deref()
            .ok_or_else(|| anyhow!("WhatsApp phone_number_id not configured"))?;

        let url = format!("{}/{}/messages", WHATSAPP_API_BASE, phone_number_id);

        let request_body = SendMessageRequest {
            messaging_product: "whatsapp".to_string(),
            recipient_type: "individual".to_string(),
            to: to.to_string(),
            message_type: "text".to_string(),
            text: SendTextContent {
                preview_url: false,
                body: text.to_string(),
            },
        };

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(access_token)
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("WhatsApp API error {}: {}", status, body));
        }

        Ok(())
    }

    /// Check if a sender (phone number) is in the allowlist.
    /// If allowlist is empty, all senders are allowed.
    fn is_sender_allowed(&self, sender_id: &str) -> bool {
        if self.config.allowlist.is_empty() {
            return true;
        }
        self.config
            .allowlist
            .iter()
            .any(|entry| entry.chat_id == sender_id)
    }

    /// Convert a WhatsApp webhook payload into InboundMessages and send them.
    pub async fn process_webhook_payload(&self, payload: WhatsAppWebhookPayload) -> Result<()> {
        if payload.object != "whatsapp_business_account" {
            return Ok(());
        }

        for entry in payload.entry {
            for change in entry.changes {
                if change.field != "messages" {
                    continue;
                }
                let value = change.value;
                let messages = match value.messages {
                    Some(m) => m,
                    None => continue,
                };

                for msg in messages {
                    // Only process text messages
                    if msg.message_type != "text" {
                        continue;
                    }
                    let text = match msg.text {
                        Some(t) => t.body,
                        None => continue,
                    };

                    let sender_id = msg.from.clone();

                    if !self.is_sender_allowed(&sender_id) {
                        warn!(
                            sender_id = %sender_id,
                            "WhatsApp: sender not in allowlist, ignoring message"
                        );
                        continue;
                    }

                    let inbound = InboundMessage {
                        channel: self.config.name.clone(),
                        sender_id: sender_id.clone(),
                        chat_id: sender_id.clone(),
                        content: text,
                        timestamp: chrono::Utc::now(),
                        media: vec![],
                        metadata: serde_json::json!({
                            "trigger_agent": true,
                            "default_agent": self.config.agent,
                            "whatsapp_message_id": msg.id,
                        }),
                    };

                    if let Err(e) = self.inbound_tx.send(inbound).await {
                        error!("Failed to forward WhatsApp message to bus: {e}");
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!(
            channel = %self.config.name,
            "WhatsApp channel starting (webhook mode)"
        );

        // Validate required config
        if self.config.access_token.is_none() {
            warn!(
                channel = %self.config.name,
                "WhatsApp access_token not configured; outbound messages will fail"
            );
        }
        if self.config.phone_number_id.is_none() {
            warn!(
                channel = %self.config.name,
                "WhatsApp phone_number_id not configured; outbound messages will fail"
            );
        }

        let retry_policy = RetryPolicy::default();

        // Spawn outbound dispatcher
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let http_client = self.http_client.clone();
        let channel_name = self.config.name.clone();
        let access_token = self.config.access_token.clone();
        let phone_number_id = self.config.phone_number_id.clone();

        tokio::spawn(async move {
            let mut retry_state = RetryState::new();

            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != channel_name {
                    continue;
                }

                let content = match &msg.message_type {
                    crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
                    crate::bus::OutboundMessageType::ToolProgress { .. } => {
                        // WhatsApp doesn't support tool progress messages
                        continue;
                    }
                    crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                        request
                            .display_message
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                            .unwrap_or_else(|| format!("Approval required: {}", request.id))
                    }
                };

                let to = msg.chat_id.clone();

                // Attempt to send with retry logic
                let mut sent = false;
                for attempt in 0..=retry_policy.max_retries {
                    if let (Some(token), Some(pn_id)) =
                        (access_token.as_deref(), phone_number_id.as_deref())
                    {
                        let url = format!("{}/{}/messages", WHATSAPP_API_BASE, pn_id);
                        let request_body = SendMessageRequest {
                            messaging_product: "whatsapp".to_string(),
                            recipient_type: "individual".to_string(),
                            to: to.clone(),
                            message_type: "text".to_string(),
                            text: SendTextContent {
                                preview_url: false,
                                body: content.clone(),
                            },
                        };

                        match http_client
                            .post(&url)
                            .bearer_auth(token)
                            .json(&request_body)
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                retry_state.reset();
                                sent = true;
                                break;
                            }
                            Ok(resp) => {
                                let status = resp.status();
                                let body = resp.text().await.unwrap_or_default();
                                let err_msg = format!("HTTP {}: {}", status, body);

                                // 401/403 are unrecoverable
                                if status.as_u16() == 401 || status.as_u16() == 403 {
                                    error!(
                                        channel = %channel_name,
                                        error = %err_msg,
                                        "WhatsApp unrecoverable auth error sending message"
                                    );
                                    break;
                                }

                                let should_retry =
                                    retry_state.record_failure(&retry_policy, err_msg.clone());
                                if should_retry && attempt < retry_policy.max_retries {
                                    let delay = retry_state.next_delay(&retry_policy);
                                    warn!(
                                        channel = %channel_name,
                                        error = %err_msg,
                                        attempt = attempt + 1,
                                        delay_ms = delay.as_millis() as u64,
                                        "WhatsApp send failed, retrying"
                                    );
                                    tokio::time::sleep(delay).await;
                                } else {
                                    error!(
                                        channel = %channel_name,
                                        error = %err_msg,
                                        "WhatsApp send failed after retries"
                                    );
                                    break;
                                }
                            }
                            Err(e) => {
                                let err_msg = format!("Request error: {e:#}");
                                let should_retry =
                                    retry_state.record_failure(&retry_policy, err_msg.clone());
                                if should_retry && attempt < retry_policy.max_retries {
                                    let delay = retry_state.next_delay(&retry_policy);
                                    warn!(
                                        channel = %channel_name,
                                        error = %err_msg,
                                        attempt = attempt + 1,
                                        delay_ms = delay.as_millis() as u64,
                                        "WhatsApp send error, retrying"
                                    );
                                    tokio::time::sleep(delay).await;
                                } else {
                                    error!(
                                        channel = %channel_name,
                                        error = %err_msg,
                                        "WhatsApp send error after retries"
                                    );
                                    break;
                                }
                            }
                        }
                    } else {
                        warn!(
                            channel = %channel_name,
                            "WhatsApp credentials not configured, cannot send message"
                        );
                        break;
                    }
                }

                if sent {
                    info!(
                        channel = %channel_name,
                        to = %to,
                        "WhatsApp message sent successfully"
                    );
                }
            }
        });

        // WhatsApp uses webhook mode — the channel itself doesn't poll.
        // The actix-web server handles incoming webhook requests and calls
        // process_webhook_payload(). This start() method just sets up the
        // outbound dispatcher and returns.
        info!(
            channel = %self.config.name,
            "WhatsApp channel started (webhook mode, waiting for inbound messages via HTTP)"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "WhatsApp channel stopping");
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let content = match &msg.message_type {
            crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
            crate::bus::OutboundMessageType::ToolProgress { .. } => return Ok(()),
            crate::bus::OutboundMessageType::ApprovalRequest { request } => request
                .display_message
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("Approval required: {}", request.id)),
        };

        self.send_whatsapp_message(&msg.chat_id, &content).await
    }
}

// ---------------------------------------------------------------------------
// WhatsApp webhook actix-web handlers
// ---------------------------------------------------------------------------

use actix_web::{web, HttpRequest, HttpResponse};

/// Query parameters for webhook verification (GET request from Meta).
#[derive(Debug, Deserialize)]
pub struct WebhookVerifyQuery {
    #[serde(rename = "hub.mode")]
    pub hub_mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub hub_verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub hub_challenge: Option<String>,
}

/// Shared state for the webhook handler.
#[derive(Clone)]
pub struct WhatsAppWebhookState {
    pub verify_token: Option<String>,
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub channel_name: String,
    pub agent: String,
    pub allowlist: Vec<AllowlistEntry>,
}

/// GET /webhook/whatsapp — Meta webhook verification handshake.
pub async fn handle_webhook_verify(
    query: web::Query<WebhookVerifyQuery>,
    state: web::Data<WhatsAppWebhookState>,
) -> HttpResponse {
    let mode = query.hub_mode.as_deref().unwrap_or("");
    let token = query.hub_verify_token.as_deref().unwrap_or("");
    let challenge = query.hub_challenge.as_deref().unwrap_or("");

    if mode == "subscribe" {
        let expected = state.verify_token.as_deref().unwrap_or("");
        if !expected.is_empty() && token == expected {
            info!("WhatsApp webhook verified successfully");
            return HttpResponse::Ok().body(challenge.to_string());
        }
        warn!("WhatsApp webhook verification failed: token mismatch");
        return HttpResponse::Forbidden().finish();
    }

    HttpResponse::BadRequest().finish()
}

/// POST /webhook/whatsapp — receive inbound messages from Meta.
pub async fn handle_webhook_post(
    _req: HttpRequest,
    body: web::Json<WhatsAppWebhookPayload>,
    state: web::Data<WhatsAppWebhookState>,
) -> HttpResponse {
    let payload = body.into_inner();

    if payload.object != "whatsapp_business_account" {
        return HttpResponse::Ok().finish();
    }

    for entry in payload.entry {
        for change in entry.changes {
            if change.field != "messages" {
                continue;
            }
            let value = change.value;
            let messages = match value.messages {
                Some(m) => m,
                None => continue,
            };

            for msg in messages {
                if msg.message_type != "text" {
                    continue;
                }
                let text = match msg.text {
                    Some(t) => t.body,
                    None => continue,
                };

                let sender_id = msg.from.clone();

                // Check allowlist
                let allowed = if state.allowlist.is_empty() {
                    true
                } else {
                    state.allowlist.iter().any(|e| e.chat_id == sender_id)
                };

                if !allowed {
                    warn!(
                        sender_id = %sender_id,
                        "WhatsApp webhook: sender not in allowlist, ignoring"
                    );
                    continue;
                }

                let inbound = InboundMessage {
                    channel: state.channel_name.clone(),
                    sender_id: sender_id.clone(),
                    chat_id: sender_id.clone(),
                    content: text,
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::json!({
                        "trigger_agent": true,
                        "default_agent": state.agent,
                        "whatsapp_message_id": msg.id,
                    }),
                };

                if let Err(e) = state.inbound_tx.send(inbound).await {
                    error!("Failed to forward WhatsApp webhook message to bus: {e}");
                }
            }
        }
    }

    // Always return 200 OK to Meta to acknowledge receipt
    HttpResponse::Ok().finish()
}

// ---------------------------------------------------------------------------
// WhatsAppChannelFactory
// ---------------------------------------------------------------------------

pub struct WhatsAppChannelFactory;

impl crate::channels::ChannelFactory for WhatsAppChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: crate::channels::ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: WhatsAppConfig = serde_json::from_value(config)
            .map_err(|e| anyhow!("Invalid WhatsApp config: {e}"))?;

        if cfg.access_token.is_none() {
            warn!("WhatsApp channel '{}' created without access_token", cfg.name);
        }
        if cfg.phone_number_id.is_none() {
            warn!(
                "WhatsApp channel '{}' created without phone_number_id",
                cfg.name
            );
        }

        let ch = WhatsAppChannel::new(cfg, ctx.inbound_tx, ctx.outbound_rx);
        Ok(Box::new(ch))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{broadcast, mpsc};

    fn make_channel() -> WhatsAppChannel {
        let (inbound_tx, _inbound_rx) = mpsc::channel(16);
        let (_outbound_tx, outbound_rx) = broadcast::channel(16);
        let config = WhatsAppConfig {
            enabled: true,
            name: "whatsapp".to_string(),
            access_token: Some("test_token".to_string()),
            phone_number_id: Some("123456789".to_string()),
            verify_token: Some("verify_secret".to_string()),
            allowlist: vec![],
            agent: "main".to_string(),
        };
        WhatsAppChannel::new(config, inbound_tx, outbound_rx)
    }

    #[test]
    fn channel_name_returns_config_name() {
        let ch = make_channel();
        assert_eq!(ch.name(), "whatsapp");
    }

    #[test]
    fn is_sender_allowed_empty_allowlist_allows_all() {
        let ch = make_channel();
        assert!(ch.is_sender_allowed("1234567890"));
        assert!(ch.is_sender_allowed("any_number"));
    }

    #[test]
    fn is_sender_allowed_with_allowlist() {
        let (inbound_tx, _) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let config = WhatsAppConfig {
            enabled: true,
            name: "whatsapp".to_string(),
            access_token: None,
            phone_number_id: None,
            verify_token: None,
            allowlist: vec![AllowlistEntry {
                chat_id: "1234567890".to_string(),
                chat_alias: "Alice".to_string(),
                my_name: None,
            }],
            agent: "main".to_string(),
        };
        let ch = WhatsAppChannel::new(config, inbound_tx, outbound_rx);
        assert!(ch.is_sender_allowed("1234567890"));
        assert!(!ch.is_sender_allowed("9999999999"));
    }

    #[tokio::test]
    async fn process_webhook_payload_sends_inbound_message() {
        let (inbound_tx, mut inbound_rx) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let config = WhatsAppConfig {
            enabled: true,
            name: "whatsapp".to_string(),
            access_token: Some("token".to_string()),
            phone_number_id: Some("123".to_string()),
            verify_token: None,
            allowlist: vec![],
            agent: "main".to_string(),
        };
        let ch = WhatsAppChannel::new(config, inbound_tx, outbound_rx);

        let payload = WhatsAppWebhookPayload {
            object: "whatsapp_business_account".to_string(),
            entry: vec![WhatsAppEntry {
                id: "entry1".to_string(),
                changes: vec![WhatsAppChange {
                    field: "messages".to_string(),
                    value: WhatsAppChangeValue {
                        messaging_product: Some("whatsapp".to_string()),
                        metadata: None,
                        contacts: None,
                        messages: Some(vec![WhatsAppMessage {
                            from: "15551234567".to_string(),
                            id: "msg1".to_string(),
                            timestamp: "1234567890".to_string(),
                            message_type: "text".to_string(),
                            text: Some(WhatsAppTextContent {
                                body: "Hello, bot!".to_string(),
                            }),
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        ch.process_webhook_payload(payload).await.unwrap();

        let received = inbound_rx.recv().await.unwrap();
        assert_eq!(received.channel, "whatsapp");
        assert_eq!(received.sender_id, "15551234567");
        assert_eq!(received.chat_id, "15551234567");
        assert_eq!(received.content, "Hello, bot!");
    }

    #[tokio::test]
    async fn process_webhook_payload_ignores_non_text_messages() {
        let (inbound_tx, mut inbound_rx) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let config = WhatsAppConfig {
            enabled: true,
            name: "whatsapp".to_string(),
            access_token: None,
            phone_number_id: None,
            verify_token: None,
            allowlist: vec![],
            agent: "main".to_string(),
        };
        let ch = WhatsAppChannel::new(config, inbound_tx, outbound_rx);

        let payload = WhatsAppWebhookPayload {
            object: "whatsapp_business_account".to_string(),
            entry: vec![WhatsAppEntry {
                id: "entry1".to_string(),
                changes: vec![WhatsAppChange {
                    field: "messages".to_string(),
                    value: WhatsAppChangeValue {
                        messaging_product: None,
                        metadata: None,
                        contacts: None,
                        messages: Some(vec![WhatsAppMessage {
                            from: "15551234567".to_string(),
                            id: "msg1".to_string(),
                            timestamp: "1234567890".to_string(),
                            message_type: "image".to_string(),
                            text: None,
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        ch.process_webhook_payload(payload).await.unwrap();

        // No message should be forwarded
        assert!(inbound_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn process_webhook_payload_ignores_blocked_senders() {
        let (inbound_tx, mut inbound_rx) = mpsc::channel(16);
        let (_, outbound_rx) = broadcast::channel(16);
        let config = WhatsAppConfig {
            enabled: true,
            name: "whatsapp".to_string(),
            access_token: None,
            phone_number_id: None,
            verify_token: None,
            allowlist: vec![AllowlistEntry {
                chat_id: "allowed_number".to_string(),
                chat_alias: "Allowed".to_string(),
                my_name: None,
            }],
            agent: "main".to_string(),
        };
        let ch = WhatsAppChannel::new(config, inbound_tx, outbound_rx);

        let payload = WhatsAppWebhookPayload {
            object: "whatsapp_business_account".to_string(),
            entry: vec![WhatsAppEntry {
                id: "entry1".to_string(),
                changes: vec![WhatsAppChange {
                    field: "messages".to_string(),
                    value: WhatsAppChangeValue {
                        messaging_product: None,
                        metadata: None,
                        contacts: None,
                        messages: Some(vec![WhatsAppMessage {
                            from: "blocked_number".to_string(),
                            id: "msg1".to_string(),
                            timestamp: "1234567890".to_string(),
                            message_type: "text".to_string(),
                            text: Some(WhatsAppTextContent {
                                body: "Hello".to_string(),
                            }),
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        ch.process_webhook_payload(payload).await.unwrap();

        // Blocked sender — no message forwarded
        assert!(inbound_rx.try_recv().is_err());
    }

    #[test]
    fn factory_creates_channel_from_valid_config() {
        use crate::channels::ChannelFactory;
        let factory = WhatsAppChannelFactory;
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
            "name": "whatsapp",
            "accessToken": "test_token",
            "phoneNumberId": "123456789",
            "verifyToken": "verify_secret",
            "allowlist": [],
            "agent": "main"
        });
        let result = factory.create(config, ctx);
        assert!(result.is_ok());
        let ch = result.unwrap();
        assert_eq!(ch.name(), "whatsapp");
    }

    #[test]
    fn factory_returns_error_for_invalid_config() {
        use crate::channels::ChannelFactory;
        let factory = WhatsAppChannelFactory;
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
        // Pass a non-object value to trigger deserialization error
        let config = serde_json::json!("not_an_object");
        let result = factory.create(config, ctx);
        assert!(result.is_err());
    }
}
