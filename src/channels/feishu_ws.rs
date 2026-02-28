//! Feishu WebSocket client using official endpoint API and local proto (Frame/Header).
//! No open-lark or lark-websocket-protobuf: get endpoint via HTTP, then connect and handle frames.

mod pbbp2 {
    include!(concat!(env!("OUT_DIR"), "/pbbp2.rs"));
}

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, trace};

pub use pbbp2::{Frame, Header};

const FEISHU_BASE_URL: &str = "https://open.feishu.cn";
const ENDPOINT_PATH: &str = "/callback/ws/endpoint";
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct WsClientConfig {
    pub ping_interval_secs: u64,
}

#[derive(Debug, Deserialize)]
struct EndpointApiResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<EndpointData>,
}

#[derive(Debug, Deserialize)]
struct EndpointData {
    #[serde(rename = "URL")]
    url: Option<String>,
    #[serde(rename = "ClientConfig")]
    client_config: Option<ClientConfigPayload>,
}

#[derive(Debug, Deserialize, Clone)]
struct ClientConfigPayload {
    #[serde(rename = "PingInterval")]
    ping_interval: Option<i32>,
}

/// POST to Feishu to get WebSocket URL and client config.
pub async fn get_ws_endpoint(
    http_client: &reqwest::Client,
    app_id: &str,
    app_secret: &str,
) -> Result<(String, WsClientConfig), String> {
    let url = format!("{FEISHU_BASE_URL}{ENDPOINT_PATH}");
    let body = serde_json::json!({
        "AppID": app_id,
        "AppSecret": app_secret
    });
    let resp = http_client
        .post(&url)
        .header("locale", "zh")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: EndpointApiResponse = resp.json().await.map_err(|e| e.to_string())?;
    if json.code != 0 {
        return Err(format!("endpoint API error {}: {}", json.code, json.msg));
    }
    let data = json.data.ok_or("endpoint response missing data")?;
    let ws_url = data.url.ok_or("endpoint response missing URL")?;
    if ws_url.is_empty() {
        return Err("endpoint URL is empty".to_string());
    }
    let ping_interval = data
        .client_config
        .as_ref()
        .and_then(|c| c.ping_interval)
        .unwrap_or(30)
        .max(5) as u64;
    let config = WsClientConfig {
        ping_interval_secs: ping_interval,
    };
    Ok((ws_url, config))
}

fn get_header(headers: &[Header], key: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| h.key == key)
        .map(|h| h.value.clone())
}

/// Build the data frame response for an event: same headers as request + biz_rt, payload = NewWsResponse JSON.
pub fn build_event_response_frame(original: &Frame, biz_rt_ms: u128) -> Frame {
    let mut headers: Vec<Header> = original
        .headers
        .iter()
        .filter(|h| h.key != "biz_rt")
        .cloned()
        .collect();
    headers.push(Header {
        key: "biz_rt".to_string(),
        value: biz_rt_ms.to_string(),
    });
    let response = serde_json::json!({
        "code": 200u16,
        "headers": { "biz_rt": biz_rt_ms.to_string() },
        "data": []
    });
    let payload = serde_json::to_vec(&response).unwrap_or_default();
    Frame {
        seq_id: original.seq_id,
        log_id: original.log_id,
        service: original.service,
        method: 1,
        headers,
        payload_encoding: original.payload_encoding.clone(),
        payload_type: original.payload_type.clone(),
        payload: Some(payload),
        log_id_new: original.log_id_new.clone(),
    }
}

fn build_ping_frame(service_id: i32) -> Frame {
    Frame {
        seq_id: 0,
        log_id: 0,
        service: service_id,
        method: 0,
        headers: vec![Header {
            key: "type".to_string(),
            value: "ping".to_string(),
        }],
        payload_encoding: None,
        payload_type: None,
        payload: None,
        log_id_new: None,
    }
}

/// Run WebSocket loop: connect, ping, handle binary frames, call on_event for "event" data frames.
/// Returns when the connection closes or errors.
pub async fn run_ws_loop<F, Fut>(
    ws_url: String,
    client_config: WsClientConfig,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(Frame) -> Fut + Send,
    Fut: std::future::Future<Output = Option<Frame>> + Send,
{
    let (stream, _) = ws_connect_async(&ws_url).await?;
    let url = url::Url::parse(&ws_url).map_err(|e| e.to_string())?;
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    let service_id: i32 = query
        .get("service_id")
        .ok_or("URL missing service_id")?
        .parse()
        .map_err(|_| "invalid service_id")?;

    let (mut sink, mut stream) = stream.split();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(client_config.ping_interval_secs));
    ping_interval.reset();
    let mut last_pong = Instant::now();
    let mut check_interval = tokio::time::interval(Duration::from_secs(1));

    info!("Feishu WebSocket connected to {}", url);

    loop {
        tokio::select! {
            msg = stream.next() => {
                let msg = msg.ok_or("stream ended")?.map_err(|e| e.to_string())?;
                match msg {
                    Message::Ping(data) => {
                        last_pong = Instant::now();
                        sink.send(Message::Pong(data)).await.map_err(|e| e.to_string())?;
                    }
                    Message::Binary(data) => {
                        let frame =
                            Frame::decode(bytes::Bytes::from(data.to_vec())).map_err(|e| e.to_string())?;
                        trace!(?frame, "Feishu WS frame");
                        match frame.method {
                            0 => {
                                let frame_type = get_header(&frame.headers, "type").unwrap_or_default();
                                if frame_type == "pong" {
                                    if let Some(ref payload) = frame.payload {
                                        if let Ok(cfg) = serde_json::from_slice::<ClientConfigPayload>(payload) {
                                            let interval_secs = cfg.ping_interval.unwrap_or(30).max(5) as u64;
                                            ping_interval = tokio::time::interval(Duration::from_secs(interval_secs));
                                            ping_interval.reset();
                                            debug!("Updated ping interval: {}s", interval_secs);
                                        }
                                    }
                                    last_pong = Instant::now();
                                }
                            }
                            1 => {
                                let msg_type = get_header(&frame.headers, "type").unwrap_or_default();
                                if msg_type == "event" {
                                    if let Some(_payload) = &frame.payload {
                                        let response = on_event(frame).await;
                                        if let Some(resp_frame) = response {
                                            let encoded = resp_frame.encode_to_vec();
                                            sink.send(Message::Binary(encoded.into())).await.map_err(|e| e.to_string())?;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Message::Close(_) => return Err("server closed connection".to_string()),
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                let frame = build_ping_frame(service_id);
                let encoded = frame.encode_to_vec();
                sink.send(Message::Binary(encoded.into())).await.map_err(|e| e.to_string())?;
            }
            _ = check_interval.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    return Err("heartbeat timeout".to_string());
                }
            }
        }
    }
}

async fn ws_connect_async(
    conn_url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    String,
> {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        ws_connect_appcontainer(conn_url).await
    } else {
        tokio_tungstenite::connect_async(conn_url)
            .await
            .map_err(|e| e.to_string())
    }
}

/// WebSocket connect when system DNS is unavailable (e.g. AppContainer): resolve via Google DNS.
#[cfg(any(windows, unix))]
pub async fn ws_connect_appcontainer(
    conn_url: &str,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    String,
> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use url::Url;

    let parsed = Url::parse(conn_url).map_err(|e| e.to_string())?;
    let host = parsed.host_str().ok_or("URL has no host")?;
    let port = parsed.port_or_known_default().ok_or("URL has no port")?;

    let ip = crate::appcontainer_dns::resolve_host(host).await?;

    let tcp = tokio::net::TcpStream::connect((ip, port))
        .await
        .map_err(|e| e.to_string())?;

    let mut request = conn_url
        .into_client_request()
        .map_err(|e| e.to_string())?;
    request
        .headers_mut()
        .insert("host", host.try_into().map_err(|_| "invalid host")?);

    tokio_tungstenite::client_async_tls_with_config(request, tcp, None, None)
        .await
        .map_err(|e| e.to_string())
}
