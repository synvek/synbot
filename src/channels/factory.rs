//! Built-in channel factories — create channel instances from config for the registry.

use std::sync::Arc;

use anyhow::Result;

use crate::channels::{
    dingtalk, discord, email, feishu, irc, matrix, slack, telegram, whatsapp, Channel, ChannelRegistry,
    ChannelStartContext,
};
use crate::config::{
    DingTalkConfig, DiscordConfig, EmailConfig, FeishuConfig, IrcConfig, MatrixConfig, SlackConfig,
    TelegramConfig, WhatsAppConfig,
};

/// Register all built-in channel factories (telegram, feishu, discord, slack, email, matrix, dingtalk, whatsapp, irc).
pub fn register_builtin_channels(registry: &mut ChannelRegistry) {
    registry.register("telegram", Arc::new(TelegramChannelFactory));
    registry.register("feishu", Arc::new(FeishuChannelFactory));
    registry.register("discord", Arc::new(DiscordChannelFactory));
    registry.register("slack", Arc::new(SlackChannelFactory));
    registry.register("email", Arc::new(EmailChannelFactory));
    registry.register("matrix", Arc::new(MatrixChannelFactory));
    registry.register("dingtalk", Arc::new(DingTalkChannelFactory));
    registry.register("whatsapp", Arc::new(WhatsAppChannelFactory));
    registry.register("irc", Arc::new(IrcChannelFactory));
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
            ctx.config_path,
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
            ctx.config_path,
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
            ctx.config_path,
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
            ctx.config_path,
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
            ctx.config_path,
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
            ctx.config_path,
        )?;
        Ok(Box::new(ch))
    }
}

struct WhatsAppChannelFactory;

impl crate::channels::ChannelFactory for WhatsAppChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: WhatsAppConfig = serde_json::from_value(config)?;
        let ch = whatsapp::WhatsAppChannel::new(cfg, ctx.inbound_tx, ctx.outbound_rx, ctx.config_path);
        Ok(Box::new(ch))
    }
}

struct IrcChannelFactory;

impl crate::channels::ChannelFactory for IrcChannelFactory {
    fn create(
        &self,
        config: serde_json::Value,
        ctx: ChannelStartContext,
    ) -> Result<Box<dyn Channel>> {
        let cfg: IrcConfig = serde_json::from_value(config)?;
        let ch = irc::IrcChannel::new(cfg, ctx.inbound_tx, ctx.outbound_rx, ctx.config_path);
        Ok(Box::new(ch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_builtin_channels_populates_registry() {
        let mut registry = ChannelRegistry::new();
        register_builtin_channels(&mut registry);
        let names = registry.type_names();
        assert!(names.contains(&"telegram".to_string()));
        assert!(names.contains(&"feishu".to_string()));
        assert!(names.contains(&"discord".to_string()));
        assert!(names.contains(&"slack".to_string()));
        assert!(names.contains(&"email".to_string()));
        assert!(names.contains(&"matrix".to_string()));
        assert!(names.contains(&"dingtalk".to_string()));
        assert!(names.contains(&"whatsapp".to_string()));
        assert!(names.contains(&"irc".to_string()));
        assert_eq!(names.len(), 9);
    }

    #[test]
    fn registry_get_returns_factory_after_register() {
        let mut registry = ChannelRegistry::new();
        register_builtin_channels(&mut registry);
        assert!(registry.get("telegram").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
