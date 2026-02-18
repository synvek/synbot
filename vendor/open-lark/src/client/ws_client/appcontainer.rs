//! Windows AppContainer WebSocket 连接支持。
//!
//! AppContainer 内系统 DNS 不可用，`tokio_tungstenite::connect_async` 会因 DNS 解析失败而报错。
//! 本模块提供 `ws_connect_appcontainer`：先用 hickory (Google DNS 8.8.8.8) 解析主机名，
//! 再建立 TCP 连接，最后完成 TLS + WebSocket 握手（保留原始 hostname 作为 SNI/Host）。
//!
//! 使用全局 `OnceLock<TokioResolver>` 单例，避免每次连接都重新初始化 resolver。

use std::sync::OnceLock;

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::TokioResolver;
use tokio::net::TcpStream;
use tokio_tungstenite::{tungstenite, MaybeTlsStream, WebSocketStream};
use url::Url;

use super::client::{WsClientError, WsClientResult};

static RESOLVER: OnceLock<TokioResolver> = OnceLock::new();

fn resolver() -> &'static TokioResolver {
    RESOLVER.get_or_init(|| {
        TokioResolver::builder_with_config(
            ResolverConfig::google(),
            TokioConnectionProvider::default(),
        )
        .build()
    })
}

/// 在 AppContainer 中建立 WebSocket 连接。
///
/// 流程：
/// 1. 用 Google DNS (8.8.8.8) 解析 hostname → IP
/// 2. 建立 TCP 连接到 IP:port
/// 3. 用原始 URL（含正确 Host/SNI）完成 TLS + WebSocket 握手
pub async fn ws_connect_appcontainer(
    conn_url: String,
) -> WsClientResult<(
    WebSocketStream<MaybeTlsStream<TcpStream>>,
    tungstenite::handshake::client::Response,
)> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let parsed = Url::parse(&conn_url).map_err(|_| WsClientError::UnexpectedResponse)?;
    let host = parsed.host_str().ok_or(WsClientError::UnexpectedResponse)?;
    let port = parsed
        .port_or_known_default()
        .ok_or(WsClientError::UnexpectedResponse)?;

    // 用 Google DNS 解析 hostname（绕过 AppContainer 内不可用的系统 DNS）
    let lookup = resolver()
        .lookup_ip(host)
        .await
        .map_err(|e| {
            WsClientError::WsError(Box::new(tungstenite::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))))
        })?;

    let ip = lookup.into_iter().next().ok_or_else(|| {
        WsClientError::WsError(Box::new(tungstenite::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("DNS: no address for {host}"),
        ))))
    })?;

    // 建立 TCP 连接
    let tcp = TcpStream::connect((ip, port))
        .await
        .map_err(|e| WsClientError::WsError(Box::new(tungstenite::Error::Io(e))))?;

    // 构造 WebSocket 请求，保留原始 hostname（正确的 Host header 和 TLS SNI）
    let mut request = conn_url
        .into_client_request()
        .map_err(WsClientError::from)?;
    request.headers_mut().insert(
        "host",
        host.parse().map_err(|_| WsClientError::UnexpectedResponse)?,
    );

    tokio_tungstenite::client_async_tls_with_config(request, tcp, None, None)
        .await
        .map_err(WsClientError::from)
}
