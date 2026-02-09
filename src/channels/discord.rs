//! Discord channel — placeholder for WebSocket-based integration.

use anyhow::Result;
use async_trait::async_trait;

use crate::bus::OutboundMessage;
use crate::channels::Channel;

pub struct DiscordChannel;

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str { "discord" }
    async fn start(&mut self) -> Result<()> {
        tracing::info!("Discord channel not yet implemented in Rust rewrite");
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> { Ok(()) }
    async fn send(&self, _msg: &OutboundMessage) -> Result<()> { Ok(()) }
}
