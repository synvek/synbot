//! WhatsApp channel — connects via **WhatsApp Web multi-device** (QR / pair code)
//! using **[wa-rs](https://crates.io/crates/wa-rs)** (stable Rust).
//!
//! Minimal integration: forwards inbound messages to the bus; outbound send is TODO.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::Channel;
use crate::config::{AllowlistEntry, WhatsAppConfig};

use wa_rs::bot::Bot;
use wa_rs::store::SqliteStore;
use wa_rs::transport::{TokioWebSocketTransportFactory, UreqHttpClient};
use wa_rs::types::events::Event;

#[derive(Clone)]
struct WhatsAppEventState {
    inbound_tx: mpsc::Sender<InboundMessage>,
    allowlist: Vec<AllowlistEntry>,
    agent: String,
    channel_name: String,
}

pub struct WhatsAppChannel {
    config: WhatsAppConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    #[allow(dead_code)]
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
}

impl WhatsAppChannel {
    pub fn new(
        config: WhatsAppConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
    ) -> Self {
        Self {
            config,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
        }
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        if self.config.session_dir.trim().is_empty() {
            warn!(
                channel = %self.config.name,
                "whatsapp session_dir is empty; pairing/connect will fail"
            );
        }

        let session_dir = std::path::PathBuf::from(&self.config.session_dir);
        if !session_dir.as_os_str().is_empty() {
            std::fs::create_dir_all(&session_dir)?;
        }
        let db_path = session_dir.join("whatsapp.db");
        let db_url = db_path.to_string_lossy().to_string();

        let backend = std::sync::Arc::new(SqliteStore::new(&db_url).await?);

        let transport = TokioWebSocketTransportFactory::new();
        let http = UreqHttpClient::new();

        let event_state = Arc::new(WhatsAppEventState {
            inbound_tx: self.inbound_tx.clone(),
            allowlist: self.config.allowlist.clone(),
            agent: self.config.agent.clone(),
            channel_name: self.config.name.clone(),
        });

        let mut bot = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport)
            .with_http_client(http)
            .on_event(move |event, _client| {
                let state = Arc::clone(&event_state);
                async move {
                    match event {
                        Event::PairingQrCode { code, .. } => {
                            info!(
                                channel = %state.channel_name,
                                "whatsapp pairing QR received:\n{}",
                                code
                            );
                        }
                        Event::PairingCode { code, .. } => {
                            info!(
                                channel = %state.channel_name,
                                "whatsapp pairing code received: {}",
                                code
                            );
                        }
                        Event::Message(msg, info) => {
                            let sender_debug = format!("{:?}", info.source.sender);
                            let sender_id: String =
                                sender_debug.chars().filter(|c| c.is_ascii_digit()).collect();

                            let allowed = state.allowlist.is_empty()
                                || state.allowlist.iter().any(|e| {
                                    e.chat_id == sender_id || e.chat_id == sender_debug
                                });
                            if !allowed {
                                warn!(
                                    channel = %state.channel_name,
                                    sender_id = %sender_id,
                                    "whatsapp: sender not in allowlist, ignoring"
                                );
                                return;
                            }

                            let content = format!("{:?}", msg);

                            let inbound = InboundMessage {
                                channel: state.channel_name.clone(),
                                sender_id: sender_id.clone(),
                                chat_id: sender_id,
                                content,
                                timestamp: chrono::Utc::now(),
                                media: vec![],
                                metadata: serde_json::json!({
                                    "trigger_agent": true,
                                    "default_agent": state.agent,
                                    "whatsapp": true
                                }),
                            };

                            if let Err(e) = state.inbound_tx.send(inbound).await {
                                warn!(
                                    channel = %state.channel_name,
                                    "whatsapp: failed to forward inbound message to bus: {e}"
                                );
                            }
                        }
                        _ => {}
                    }
                }
            })
            .build()
            .await?;

        info!(
            channel = %self.config.name,
            "whatsapp bot started; waiting for pairing and inbound messages"
        );

        let running = bot.run().await?;
        running.await?;

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "whatsapp stopping");
        Ok(())
    }

    async fn send(&self, _msg: &OutboundMessage) -> Result<()> {
        warn!(
            channel = %self.config.name,
            "whatsapp send() not implemented yet (outbound messages will be dropped)"
        );
        Ok(())
    }
}
