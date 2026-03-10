//! DingTalk Stream protocol — self-implemented (no dingtalk-stream crate).
//!
//! Flow: POST connections/open -> endpoint + ticket -> WebSocket with ticket -> text JSON frames;
//! respond with ACK JSON per open.dingtalk.com stream protocol.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

const OPEN_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
const BOT_TOPIC: &str = "/v1.0/im/bot/messages/get";

#[derive(Debug, Deserialize)]
struct OpenResponse {
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    ticket: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushEnvelope {
    #[serde(default)]
    spec_version: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    headers: Option<serde_json::Value>,
    #[serde(default)]
    data: Option<String>,
}

/// Register Stream connection; returns WebSocket base URL and one-time ticket (90s, single use).
pub async fn open_connection(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
) -> Result<(String, String), String> {
    let client_id = client_id.trim();
    let client_secret = client_secret.trim();
    if client_id.is_empty() || client_secret.is_empty() {
        return Err("clientId and clientSecret (or appKey/appSecret) must be non-empty".into());
    }
    let body = json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "subscriptions": [
            { "type": "CALLBACK", "topic": BOT_TOPIC }
        ],
        "ua": concat!("synbot-rust/", env!("CARGO_PKG_VERSION"))
    });
    let resp = http
        .post(OPEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let hint = if status.as_u16() == 401 || text.contains("authFailed") || text.contains("鉴权失败") {
            " (Check: 应用开发 → 应用 → 应用信息 中的 ClientID/ClientSecret 或 AppKey/AppSecret 是否正确；是否已为该应用开启 Stream 模式与机器人能力；同一应用是否被其他进程占用连接)"
        } else {
            ""
        };
        return Err(format!("connections/open failed {status}: {text}{hint}"));
    }
    let v: OpenResponse = resp.json().await.map_err(|e| e.to_string())?;
    if v.endpoint.is_empty() || v.ticket.is_empty() {
        return Err("connections/open response missing endpoint or ticket".into());
    }
    Ok((v.endpoint, v.ticket))
}

/// Build WebSocket URL from endpoint and ticket.
pub fn ws_url(endpoint: &str, ticket: &str) -> String {
    let base = endpoint.trim();
    if let Ok(mut u) = url::Url::parse(base) {
        u.query_pairs_mut().append_pair("ticket", ticket);
        return u.to_string();
    }
    // Fallback if endpoint is not a full URL
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{}{}ticket={}", base.trim_end_matches('/'), sep, ticket)
}

fn ack_bot_callback(message_id: &str) -> String {
    json!({
        "code": 200,
        "message": "OK",
        "headers": {
            "messageId": message_id,
            "contentType": "application/json"
        },
        "data": "{\"response\": null}"
    })
    .to_string()
}

fn ack_ping(message_id: &str, opaque: &str) -> String {
    let data = serde_json::to_string(&json!({ "opaque": opaque })).unwrap_or_else(|_| "{}".into());
    json!({
        "code": 200,
        "message": "OK",
        "headers": {
            "messageId": message_id,
            "contentType": "application/json"
        },
        "data": data
    })
    .to_string()
}

fn ack_event_success(message_id: &str) -> String {
    json!({
        "code": 200,
        "message": "OK",
        "headers": {
            "messageId": message_id,
            "contentType": "application/json"
        },
        "data": "{\"status\": \"SUCCESS\", \"message\": \"ok\"}"
    })
    .to_string()
}

async fn ws_connect(conn_url: &str) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    String,
> {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        crate::channels::feishu_ws::ws_connect_appcontainer(conn_url).await
    } else {
        tokio_tungstenite::connect_async(conn_url)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Run one WebSocket session until disconnect or error.
/// `on_bot_message` receives the inner JSON string from CALLBACK data; should return quickly (spawn if needed).
pub async fn run_ws_session<F>(ws_url: String, mut on_bot_message: F) -> Result<(), String>
where
    F: FnMut(String) + Send,
{
    let (ws_stream, _) = ws_connect(&ws_url).await?;
    let (mut write, mut read) = ws_stream.split();
    info!("DingTalk Stream WebSocket connected");

    while let Some(msg) = read.next().await {
        let msg = msg.map_err(|e| e.to_string())?;
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Ping(p) => {
                let _ = write.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(_) => {
                warn!("DingTalk Stream WebSocket closed by server");
                return Ok(());
            }
            _ => continue,
        };

        let env: PushEnvelope = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(e) => {
                debug!(error = %e, "DingTalk Stream non-JSON frame");
                continue;
            }
        };

        let headers = env.headers.as_ref().and_then(|h| h.as_object());
        let topic = headers
            .and_then(|h| h.get("topic"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let message_id = headers
            .and_then(|h| h.get("messageId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let push_type = env.r#type.as_deref().unwrap_or("");

        match push_type {
            "SYSTEM" => {
                if topic == "ping" {
                    let opaque = env
                        .data
                        .as_ref()
                        .and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
                        .and_then(|v| v.get("opaque").and_then(|x| x.as_str()).map(|s| s.to_string()))
                        .unwrap_or_default();
                    let ack = ack_ping(message_id, &opaque);
                    write
                        .send(Message::Text(ack.into()))
                        .await
                        .map_err(|e| e.to_string())?;
                } else if topic == "disconnect" {
                    // No ACK; server closes in ~10s — exit so outer loop reopens.
                    info!("DingTalk Stream disconnect notice; will reconnect after delay");
                    tokio::time::sleep(Duration::from_secs(11)).await;
                    return Ok(());
                }
            }
            "CALLBACK" if topic == BOT_TOPIC => {
                if let Some(data) = env.data.clone() {
                    on_bot_message(data);
                }
                let ack = ack_bot_callback(message_id);
                write
                    .send(Message::Text(ack.into()))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "EVENT" => {
                let ack = ack_event_success(message_id);
                write
                    .send(Message::Text(ack.into()))
                    .await
                    .map_err(|e| e.to_string())?;
            }
            _ => {
                if !message_id.is_empty() {
                    let ack = ack_bot_callback(message_id);
                    let _ = write.send(Message::Text(ack.into())).await;
                }
            }
        }
    }

    Ok(())
}

/// Long-running loop: open -> connect -> run session -> backoff on error.
pub async fn run_forever<F>(http: reqwest::Client, client_id: String, client_secret: String, mut on_bot_message: F)
where
    F: FnMut(String) + Send + 'static,
{
    let policy = crate::channels::RetryPolicy::default();
    let mut attempt = 0u32;
    loop {
        match open_connection(&http, &client_id, &client_secret).await {
            Ok((endpoint, ticket)) => {
                attempt = 0;
                let url = ws_url(&endpoint, &ticket);
                match run_ws_session(url, |data| on_bot_message(data)).await {
                    Ok(()) => {
                        // disconnect or clean close — short delay then reopen
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "DingTalk Stream session error");
                        let delay = policy.delay_for_attempt(attempt);
                        attempt = (attempt + 1).min(policy.max_retries);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "DingTalk Stream open_connection failed");
                let delay = policy.delay_for_attempt(attempt);
                attempt = (attempt + 1).min(policy.max_retries);
                tokio::time::sleep(delay).await;
            }
        }
    }
}
