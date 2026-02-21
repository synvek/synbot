//! DNS resolver for app sandbox (Windows AppContainer and macOS nono/Seatbelt).
//!
//! In restricted sandboxes, system DNS may be unavailable or blocked:
//! - Windows AppContainer: `GetAdaptersAddresses` returns an empty nameserver list.
//! - macOS Seatbelt: system resolver (e.g. mDNSResponder) may be denied.
//!
//! This module provides:
//! - `GoogleDnsResolver`: implements reqwest's `Resolve` trait using Google DNS (8.8.8.8) directly.
//! - `global_resolver()`: global singleton `TokioResolver` for non-reqwest paths (e.g. WebSocket).
//! - `build_reqwest_client()`: when `SYNBOT_IN_APP_SANDBOX` is set, returns a `reqwest::Client`
//!   with `GoogleDnsResolver` injected; otherwise returns the default client.
//!
//! Usage:
//! ```rust
//! // HTTP (reqwest)
//! let client = appcontainer_dns::build_reqwest_client();
//!
//! // WebSocket (tokio-tungstenite) – open-lark uses this when SYNBOT_IN_APP_SANDBOX is set.
//! let ip = appcontainer_dns::resolve_host("open.feishu.cn").await?;
//! ```
//!
//! For third-party crates that cannot accept an injected reqwest client (e.g. rig-core),
//! DNS issues must be addressed by other means (e.g. vendoring the crate and injecting
//! a client, or waiting for upstream support).

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::TokioResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

// ── Global resolver singleton ───────────────────────────────────────────────────

static GLOBAL_RESOLVER: OnceLock<TokioResolver> = OnceLock::new();

/// Returns the global Google DNS resolver singleton (8.8.8.8).
///
/// Initialized on first call; subsequent calls return the same instance.
pub fn global_resolver() -> &'static TokioResolver {
    GLOBAL_RESOLVER.get_or_init(|| {
        TokioResolver::builder_with_config(
            ResolverConfig::google(),
            TokioConnectionProvider::default(),
        )
        .build()
    })
}

/// Resolves a hostname and returns the first IP address. For non-reqwest paths (e.g. WebSocket).
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

/// reqwest `Resolve` implementation using the global Google DNS resolver.
///
/// Injected via `reqwest::ClientBuilder::dns_resolver(Arc::new(GoogleDnsResolver))` so that
/// all requests from that client use Google DNS, bypassing the unavailable system DNS in AppContainer.
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

/// Builds a `reqwest::Client`.
///
/// - Inside app sandbox (when `SYNBOT_IN_APP_SANDBOX` is set): injects `GoogleDnsResolver`
///   and on macOS uses rustls with webpki-roots only (no system certs) to avoid Security
///   framework / OSStatus -9808 in Seatbelt.
/// - Otherwise: returns the default client.
///
/// Callers can use this function directly without conditional compilation.
pub fn build_reqwest_client() -> reqwest::Client {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        let mut b = reqwest::Client::builder().dns_resolver(Arc::new(GoogleDnsResolver));
        #[cfg(target_os = "macos")]
        {
            b = b
                .use_rustls_tls()
                .tls_built_in_root_certs(false)
                .tls_built_in_webpki_certs(true);
        }
        b.build().unwrap_or_default()
    } else {
        reqwest::Client::new()
    }
}

/// Builds a `reqwest::Client` with a custom default User-Agent (same DNS/TLS behavior as `build_reqwest_client`).
pub fn build_reqwest_client_with_user_agent(user_agent: &str) -> reqwest::Client {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        let mut b = reqwest::Client::builder()
            .dns_resolver(Arc::new(GoogleDnsResolver))
            .user_agent(user_agent);
        #[cfg(target_os = "macos")]
        {
            b = b
                .use_rustls_tls()
                .tls_built_in_root_certs(false)
                .tls_built_in_webpki_certs(true);
        }
        b.build().unwrap_or_default()
    } else {
        reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .unwrap_or_default()
    }
}

/// Builds a `reqwest::Client` with a timeout (Google DNS and macOS rustls/webpki same as above).
pub fn build_reqwest_client_with_timeout(timeout: std::time::Duration) -> reqwest::Client {
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        let mut b = reqwest::Client::builder()
            .dns_resolver(Arc::new(GoogleDnsResolver))
            .timeout(timeout);
        #[cfg(target_os = "macos")]
        {
            b = b
                .use_rustls_tls()
                .tls_built_in_root_certs(false)
                .tls_built_in_webpki_certs(true);
        }
        b.build().unwrap_or_default()
    } else {
        reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default()
    }
}
