//! Feishu channel — WebSocket long-connection based.
//!
//! Uses the `open-lark` SDK's WebSocket client to maintain a persistent
//! connection with Feishu, receiving messages via event subscription and
//! sending replies through the IM v1 message API.
//!
//! Note: `EventDispatcherHandler` from open-lark is `!Send`, so the
//! WebSocket event loop runs on a dedicated single-threaded tokio runtime.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use open_lark::client::ws_client::LarkWsClient;
use open_lark::prelude::*;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::Channel;
use crate::config::FeishuConfig;

pub struct FeishuChannel {
    config: FeishuConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    running: bool,
}

impl FeishuChannel {
    pub fn new(
        config: FeishuConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
    ) -> Self {
        Self {
            config,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            running: false,
        }
    }

    /// Build a LarkClient for API calls (bot info, send messages).
    fn build_lark_client(&self) -> LarkClient {
        LarkClient::builder(&self.config.app_id, &self.config.app_secret)
            .with_app_type(AppType::SelfBuild)
            .with_enable_token_cache(true)
            .build()
    }

    /// Send a text message to a chat via the IM v1 API.
    async fn send_text(client: &LarkClient, chat_id: &str, text: &str) -> Result<()> {
        // Feishu text message limit is ~150KB; split at a safe boundary.
        const CHUNK_SIZE: usize = 30_000;
        let chunks: Vec<&str> = if text.len() <= CHUNK_SIZE {
            vec![text]
        } else {
            text.as_bytes()
                .chunks(CHUNK_SIZE)
                .map(|c| std::str::from_utf8(c).unwrap_or(""))
                .collect()
        };

        for chunk in chunks {
            let content = serde_json::json!({ "text": chunk }).to_string();
            let body = CreateMessageRequestBody::builder()
                .receive_id(chat_id)
                .msg_type("text")
                .content(content)
                .build();
            let req = CreateMessageRequest::builder()
                .receive_id_type("chat_id")
                .request_body(body)
                .build();

            if let Err(e) = client.im.v1.message.create(req, None).await {
                error!("Feishu send_text error: {e:#}");
                return Err(e.into());
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Channel for FeishuChannel {
    fn name(&self) -> &str {
        "feishu"
    }

    async fn start(&mut self) -> Result<()> {
        info!("Feishu channel starting (WebSocket long-connection)");
        self.running = true;

        let client = self.build_lark_client();

        // --- Verify bot credentials ---
        match client.bot.v3.info.get(None).await {
            Ok(response) => {
                if let Some(data) = response.data {
                    info!("Feishu bot connected successfully");
                    if let Some(name) = &data.bot.app_name {
                        info!("  Bot name: {name}");
                    }
                    if let Some(open_id) = &data.bot.open_id {
                        info!("  Open ID: {open_id}");
                    }
                } else {
                    warn!("Feishu bot info response contained no data");
                }
            }
            Err(e) => {
                error!("Failed to fetch Feishu bot info: {e:?}");
            }
        }

        // --- Spawn outbound message dispatcher ---
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let outbound_client = self.build_lark_client();
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != "feishu" {
                    continue;
                }
                if let Err(e) =
                    FeishuChannel::send_text(&outbound_client, &msg.chat_id, &msg.content).await
                {
                    error!("Feishu outbound send error: {e:#}");
                }
            }
        });

        // --- Start WebSocket on a dedicated thread ---
        // EventDispatcherHandler is !Send, so we run the WS event loop
        // on a single-threaded tokio runtime in a separate OS thread.
        let inbound_tx = self.inbound_tx.clone();
        let allow_from = self.config.allow_from.clone();
        let app_id = self.config.app_id.clone();
        let app_secret = self.config.app_secret.clone();

        let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build Feishu WS runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let handler = EventDispatcherHandler::builder()
                    .register_p2_im_message_receive_v1(move |event| {
                        info!("Feishu event callback fired!");
                        let sender_open_id =
                            event.event.sender.sender_id.open_id.clone();
                        info!(
                            sender = %sender_open_id,
                            "Feishu received message from sender"
                        );

                        // Access control
                        if !allow_from.is_empty()
                            && !allow_from.iter().any(|a| a == &sender_open_id)
                        {
                            warn!(sender = %sender_open_id, "Feishu access denied");
                            return;
                        }

                        let msg = &event.event.message;
                        info!(
                            message_type = %msg.message_type,
                            chat_id = %msg.chat_id,
                            content = %msg.content,
                            "Feishu message detail"
                        );

                        // Extract text; for non-text messages forward raw content
                        let text = if msg.message_type == "text" {
                            serde_json::from_str::<serde_json::Value>(&msg.content)
                                .ok()
                                .and_then(|v| {
                                    v.get("text").and_then(|t| t.as_str().map(String::from))
                                })
                                .unwrap_or_default()
                        } else {
                            msg.content.clone()
                        };

                        if text.is_empty() {
                            warn!("Feishu message text is empty, skipping");
                            return;
                        }

                        let inbound = InboundMessage {
                            channel: "feishu".into(),
                            sender_id: sender_open_id,
                            chat_id: msg.chat_id.clone(),
                            content: text,
                            timestamp: chrono::Utc::now(),
                            media: vec![],
                            metadata: serde_json::json!({
                                "message_id": msg.message_id,
                                "message_type": msg.message_type,
                                "chat_type": msg.chat_type,
                            }),
                        };

                        // Use try_send to avoid needing async context.
                        // The mpsc channel has capacity 256, so this
                        // should not fail under normal conditions.
                        match inbound_tx.try_send(inbound) {
                            Ok(()) => info!("Feishu inbound message forwarded to bus"),
                            Err(e) => error!("Failed to forward Feishu inbound message: {e}"),
                        }
                    })
                    .expect("Failed to register im.message.receive_v1 handler")
                    .build();

                let lark_config = Arc::new(
                    open_lark::core::config::Config::builder()
                        .app_id(&app_id)
                        .app_secret(&app_secret)
                        .req_timeout(std::time::Duration::from_secs(30))
                        .build(),
                );

                info!("Feishu WebSocket connecting...");
                let result = LarkWsClient::open(lark_config, handler).await;
                match &result {
                    Ok(()) => {
                        info!("Feishu WebSocket connection closed normally");
                        let _ = result_tx.send(Ok(()));
                    }
                    Err(e) => {
                        error!("Feishu WebSocket error: {e:#}");
                        let _ = result_tx.send(Err(format!("{e}")));
                    }
                }
            });
        });

        // Wait for the WS connection to finish (it blocks until disconnect)
        match result_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow::anyhow!("Feishu WebSocket error: {e}")),
            Err(_) => Err(anyhow::anyhow!("Feishu WebSocket thread terminated unexpectedly")),
        }
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Feishu channel stopping");
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let client = self.build_lark_client();
        FeishuChannel::send_text(&client, &msg.chat_id, &msg.content).await
    }
}
