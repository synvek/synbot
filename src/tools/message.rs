//! Message tool — send outbound messages to channels (text and optional files).
//!
//! When the user asks the agent to send files (e.g. on Feishu), the agent can call
//! this tool with both `content` and `files` (paths relative to workspace). No separate
//! "send_file" tool is needed.

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
    fn description(&self) -> &str {
        "Send a message to the user on the current chat channel. Optional: pass 'files' (array of file paths relative to workspace) to attach files to the message (e.g. Feishu/Discord/Slack will send them). Use when the user asks you to send or share a file."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Text to send" },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional: paths of files to attach (relative to workspace), e.g. [\"report.pdf\", \"image.png\"]"
                },
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
        let media: Vec<String> = args["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let media_len = media.len();
        let empty_media = media.is_empty();
        // When files are present, do not send here: the agent loop will attach them to the final
        // reply and send one message (avoids duplicate and ensures channels like DingTalk get
        // text + files in a single outbound message).
        if empty_media {
            let msg = OutboundMessage::chat(channel, chat_id, content, media, None);
            let _ = self.outbound_tx.send(msg);
        }
        if empty_media {
            Ok("Message sent.".into())
        } else {
            Ok(format!("Message sent with {} file(s) (will be delivered with your next reply).", media_len))
        }
    }
}
