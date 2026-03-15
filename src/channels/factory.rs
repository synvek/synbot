//! Built-in channel factories — create channel instances from config for the registry.

use std::sync::Arc;

use anyhow::Result;

use crate::channels::{
    dingtalk, discord, email, feishu, matrix, slack, telegram, Channel, ChannelRegistry,
    ChannelStartContext,
};
use crate::config::{
    DingTalkConfig, DiscordConfig, EmailConfig, FeishuConfig, MatrixConfig, SlackConfig,
    TelegramConfig,
};

/// Register all built-in channel factories (telegram, feishu, discord, slack, email, matrix, dingtalk).
pub fn register_builtin_channels(registry: &mut ChannelRegistry) {
    registry.register("telegram", Arc::new(TelegramChannelFactory));
    registry.register("feishu", Arc::new(FeishuChannelFactory));
    registry.register("discord", Arc::new(DiscordChannelFactory));
    registry.register("slack", Arc::new(SlackChannelFactory));
    registry.register("email", Arc::new(EmailChannelFactory));
    registry.register("matrix", Arc::new(MatrixChannelFactory));
    registry.register("dingtalk", Arc::new(DingTalkChannelFactory));
}

struct DingTalkChannelFactory;

impl crate::channels::ChannelFactory for DingTalkChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: DingTalkConfig = serde_json::from_value(config)?;
        let ch = dingtalk::DingTalkChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
            ctx.workspace,
        );
        Ok(Box::new(ch))
    }
}

struct TelegramChannelFactory;

impl crate::channels::ChannelFactory for TelegramChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: TelegramConfig = serde_json::from_value(config)?;
        let ch = telegram::TelegramChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
        );
        Ok(Box::new(ch))
    }
}

struct FeishuChannelFactory;

impl crate::channels::ChannelFactory for FeishuChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: FeishuConfig = serde_json::from_value(config)?;
        let mut ch = feishu::FeishuChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
            ctx.workspace,
        );
        if let Some(tx) = ctx.outbound_tx {
            ch = ch.with_outbound_tx(tx);
        }
        if let Some(mgr) = ctx.approval_manager {
            ch = ch.with_approval_manager(mgr);
        }
        if let Some(model) = ctx.completion_model {
            ch = ch.with_approval_classifier(model);
        }
        Ok(Box::new(ch))
    }
}

struct DiscordChannelFactory;

impl crate::channels::ChannelFactory for DiscordChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: DiscordConfig = serde_json::from_value(config)?;
        let ch = discord::DiscordChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
            ctx.workspace,
        );
        Ok(Box::new(ch))
    }
}

struct SlackChannelFactory;

impl crate::channels::ChannelFactory for SlackChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: SlackConfig = serde_json::from_value(config)?;
        let ch = slack::SlackChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
            ctx.workspace,
        )?;
        Ok(Box::new(ch))
    }
}

struct EmailChannelFactory;

impl crate::channels::ChannelFactory for EmailChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: EmailConfig = serde_json::from_value(config)?;
        let ch = email::EmailChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
        );
        Ok(Box::new(ch))
    }
}

struct MatrixChannelFactory;

impl crate::channels::ChannelFactory for MatrixChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: MatrixConfig = serde_json::from_value(config)?;
        let ch = matrix::MatrixChannel::new(
            cfg,
            ctx.inbound_tx,
            ctx.outbound_rx,
            ctx.show_tool_calls,
            ctx.tool_result_preview_chars,
            ctx.workspace,
        )?;
        Ok(Box::new(ch))
    }
}
