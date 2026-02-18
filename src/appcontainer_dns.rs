//! DNS resolver for Windows AppContainer.
//!
//! AppContainer 内系统 DNS 配置不可读（`GetAdaptersAddresses` 返回空 nameserver 列表），
//! 导致 hickory-dns 报 "no connections available"。
//!
//! 本模块提供：
//! - `GoogleDnsResolver`：实现 reqwest 的 `Resolve` trait，直接使用 Google DNS (8.8.8.8)。
//! - `global_resolver()`：全局单例 `TokioResolver`，供 WebSocket 等非 reqwest 路径使用。
//! - `build_reqwest_client()`：在 AppContainer 中返回注入了 `GoogleDnsResolver` 的
//!   `reqwest::Client`，否则返回默认 client。调用方无需关心是否在 AppContainer 中。
//!
//! 使用方式：
//! ```rust
//! // HTTP（reqwest）
//! let client = appcontainer_dns::build_reqwest_client();
//!
//! // WebSocket（tokio-tungstenite）
//! let ip = appcontainer_dns::resolve_host("open.feishu.cn").await?;
//! ```
//!
//! 对于无法注入 reqwest client 的第三方库（如 rig-core），DNS 问题需通过其他途径解决
//! （例如 vendor 该库并注入 client，或等待上游支持）。

#![cfg(target_os = "windows")]

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::TokioResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

// ── 全局 resolver 单例 ────────────────────────────────────────────────────────

static GLOBAL_RESOLVER: OnceLock<TokioResolver> = OnceLock::new();

/// 获取全局 Google DNS resolver 单例（8.8.8.8）。
///
/// 首次调用时初始化，后续调用直接返回已有实例。
pub fn global_resolver() -> &'static TokioResolver {
    GLOBAL_RESOLVER.get_or_init(|| {
        TokioResolver::builder_with_config(
            ResolverConfig::google(),
            TokioConnectionProvider::default(),
        )
        .build()
    })
}

/// 解析主机名，返回第一个 IP 地址。供 WebSocket 等非 reqwest 路径使用。
pub async fn resolve_host(host: &str) -> Result<IpAddr, String> {
    let lookup = global_resolver()
        .lookup_ip(host)
        .await
        .map_err(|e| e.to_string())?;
    lookup
        .into_iter()
        .next()
        .ok_or_else(|| format!("DNS: no address for {host}"))
}

// ── reqwest DNS resolver ──────────────────────────────────────────────────────

/// reqwest `Resolve` 实现，使用全局 Google DNS resolver。
///
/// 通过 `reqwest::ClientBuilder::dns_resolver(Arc::new(GoogleDnsResolver))` 注入，
/// 使该 client 的所有请求都走 Google DNS，绕过 AppContainer 内不可用的系统 DNS。
#[derive(Clone)]
pub struct GoogleDnsResolver;

impl Resolve for GoogleDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let lookup = global_resolver()
                .lookup_ip(name.as_str())
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    e.to_string().into()
                })?;
            let addrs: Addrs =
                Box::new(lookup.into_iter().map(|addr| SocketAddr::new(addr, 0)));
            Ok(addrs)
        })
    }
}

// ── reqwest client builder ────────────────────────────────────────────────────

/// 构建 `reqwest::Client`。
///
/// - 在 AppContainer 中（`SYNBOT_IN_APP_SANDBOX` 已设置）：注入 `GoogleDnsResolver`，
///   使所有 HTTP 请求走 Google DNS (8.8.8.8)。
/// - 其他环境：返回默认 client。
///
/// 调用方无需条件编译，直接使用此函数即可。
pub fn build_reqwest_client() -> reqwest::Client {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        reqwest::Client::builder()
            .dns_resolver(Arc::new(GoogleDnsResolver))
            .build()
            .unwrap_or_default()
    } else {
        reqwest::Client::new()
    }
}

/// 构建带超时的 `reqwest::Client`（AppContainer 中同样注入 Google DNS）。
pub fn build_reqwest_client_with_timeout(timeout: std::time::Duration) -> reqwest::Client {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        reqwest::Client::builder()
            .dns_resolver(Arc::new(GoogleDnsResolver))
            .timeout(timeout)
            .build()
            .unwrap_or_default()
    } else {
        reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default()
    }
}
