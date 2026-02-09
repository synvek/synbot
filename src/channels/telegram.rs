//! Telegram channel — long-polling based.
//!
//! Uses the Telegram Bot API directly via reqwest (no heavy SDK dependency).

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::Channel;
use crate::config::TelegramConfig;

const API_BASE: &str = "https://api.telegram.org/bot";

pub struct TelegramChannel {
    config: TelegramConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    client: reqwest::Client,
    running: bool,
}

#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
struct TgMessage {
    message_id: i64,
    from: Option<TgUser>,
    chat: TgChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgUser {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
}

impl TelegramChannel {
    pub fn new(
        config: TelegramConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            client,
            running: false,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}{}/{}", API_BASE, self.config.token, method)
    }

    async fn poll_updates(&self, offset: i64) -> Result<Vec<TgUpdate>> {
        let resp: TgResponse<Vec<TgUpdate>> = self
            .client
            .get(self.api_url("getUpdates"))
            .query(&[("offset", offset), ("timeout", 30)])
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.result.unwrap_or_default())
    }

    async fn send_text(&self, chat_id: i64, text: &str) -> Result<()> {
        // Telegram limits messages to 4096 chars; split if needed.
        for chunk in text.as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            self.client
                .post(self.api_url("sendMessage"))
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": chunk_str,
                    "parse_mode": "HTML"
                }))
                .send()
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str { "telegram" }

    async fn start(&mut self) -> Result<()> {
        info!("Telegram channel starting (long-polling)");
        self.running = true;
        let mut offset: i64 = 0;

        // Spawn outbound dispatcher
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let client = self.client.clone();
        let token = self.config.token.clone();
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != "telegram" {
                    continue;
                }
                if let Ok(chat_id) = msg.chat_id.parse::<i64>() {
                    let url = format!("{}{}/sendMessage", API_BASE, token);
                    for chunk in msg.content.as_bytes().chunks(4000) {
                        let chunk_str = String::from_utf8_lossy(chunk);
                        let _ = client
                            .post(&url)
                            .json(&serde_json::json!({
                                "chat_id": chat_id,
                                "text": chunk_str,
                                "parse_mode": "HTML"
                            }))
                            .send()
                            .await;
                    }
                }
            }
        });

        // Poll loop
        while self.running {
            match self.poll_updates(offset).await {
                Ok(updates) => {
                    for u in updates {
                        offset = u.update_id + 1;
                        if let Some(m) = u.message {
                            let sender = m.from.map(|u| u.id.to_string()).unwrap_or_default();
                            if !self.is_allowed(&sender, &self.config.allow_from) {
                                warn!(sender, "Access denied");
                                continue;
                            }
                            if let Some(text) = m.text {
                                let _ = self.inbound_tx.send(InboundMessage {
                                    channel: "telegram".into(),
                                    sender_id: sender,
                                    chat_id: m.chat.id.to_string(),
                                    content: text,
                                    timestamp: chrono::Utc::now(),
                                    media: vec![],
                                    metadata: serde_json::Value::Null,
                                }).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Telegram poll error: {e:#}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let chat_id: i64 = msg.chat_id.parse()?;
        self.send_text(chat_id, &msg.content).await
    }
}
