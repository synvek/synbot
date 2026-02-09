pub mod telegram;
pub mod discord;
pub mod feishu;

use anyhow::Result;
use async_trait::async_trait;

use crate::bus::OutboundMessage;

/// Trait that all channel implementations must satisfy.
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;

    /// Check if a sender is in the allow-list. Empty list = allow all.
    fn is_allowed(&self, sender_id: &str, allow_list: &[String]) -> bool {
        if allow_list.is_empty() {
            return true;
        }
        allow_list.iter().any(|a| a == sender_id)
    }
}
