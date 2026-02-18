//! DNS resolver for Windows AppContainer: uses Google DNS (8.8.8.8) so we don't rely
//! on system DNS config (unreadable in AppContainer → "no connections available").
//!
//! Only used when `SYNBOT_IN_APP_SANDBOX` is set; the startup diagnostic client
//! uses this resolver. Feishu/open-lark still use their default resolver.

#![cfg(target_os = "windows")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::OnceLock;

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::TokioResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// Resolver that uses Google DNS (8.8.8.8) so DNS works in AppContainer without
/// reading system config. Implements reqwest's `Resolve` for use with
/// `ClientBuilder::dns_resolver(Arc::new(GoogleDnsResolver::new()))`.
#[derive(Clone)]
pub struct GoogleDnsResolver {
    state: Arc<OnceLock<TokioResolver>>,
}

impl GoogleDnsResolver {
    pub fn new() -> Self {
        Self {
            state: Arc::new(OnceLock::new()),
        }
    }

    fn get_resolver(&self) -> &TokioResolver {
        self.state.get_or_init(|| {
            hickory_resolver::Resolver::builder_with_config(
                ResolverConfig::google(),
                TokioConnectionProvider::default(),
            )
            .build()
        })
    }
}

impl Default for GoogleDnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolve for GoogleDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let this = self.clone();
        Box::pin(async move {
            let resolver = this.get_resolver();
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
            let addrs: Addrs =
                Box::new(lookup.into_iter().map(|addr| SocketAddr::new(addr, 0)));
            Ok(addrs)
        })
    }
}
