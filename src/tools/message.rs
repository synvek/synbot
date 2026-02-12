//! Message tool — send outbound messages to channels.

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::bus::OutboundMessage;
use crate::tools::DynTool;

pub struct MessageTool {
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub default_channel: String,
    pub default_chat_id: String,
}

#[async_trait::async_trait]
impl DynTool for MessageTool {
    fn name(&self) -> &str { "message" }
    fn description(&self) -> &str { "Send a message to the user on a chat channel." }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "channel": { "type": "string" },
                "chat_id": { "type": "string" }
            },
            "required": ["content"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let content = args["content"].as_str().unwrap_or("").to_string();
        let channel = args["channel"]
            .as_str()
            .unwrap_or(&self.default_channel)
            .to_string();
        let chat_id = args["chat_id"]
            .as_str()
            .unwrap_or(&self.default_chat_id)
            .to_string();

        let msg = OutboundMessage::chat(
            channel,
            chat_id,
            content,
            vec![],
            None,
        );
        let _ = self.outbound_tx.send(msg);
        Ok("Message sent.".into())
    }
}
