//! Async message bus — decouples channels from the agent core.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use crate::tools::approval::{ApprovalRequest, ApprovalResponse};

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub media: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl InboundMessage {
    pub fn session_key(&self) -> String {
        format!("{}:{}", self.channel, self.chat_id)
    }
}

/// Approval request (agent → user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestMessage {
    pub request: ApprovalRequest,
}

/// Approval response (user → agent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponseMessage {
    pub response: ApprovalResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundMessageType {
    Chat {
        content: String,
        #[serde(default)]
        media: Vec<String>,
    },
    ApprovalRequest {
        request: ApprovalRequest,
    },
    ToolProgress {
        tool_name: String,
        status: String,
        result_preview: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    #[serde(flatten)]
    pub message_type: OutboundMessageType,
    pub reply_to: Option<String>,
}

impl OutboundMessage {
    pub fn chat(
        channel: String,
        chat_id: String,
        content: String,
        media: Vec<String>,
        reply_to: Option<String>,
    ) -> Self {
        Self {
            channel,
            chat_id,
            message_type: OutboundMessageType::Chat { content, media },
            reply_to,
        }
    }

    pub fn approval_request(
        channel: String,
        chat_id: String,
        request: ApprovalRequest,
        reply_to: Option<String>,
    ) -> Self {
        Self {
            channel,
            chat_id,
            message_type: OutboundMessageType::ApprovalRequest { request },
            reply_to,
        }
    }

    pub fn tool_progress(
        channel: String,
        chat_id: String,
        tool_name: String,
        status: String,
        result_preview: String,
    ) -> Self {
        Self {
            channel,
            chat_id,
            message_type: OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            },
            reply_to: None,
        }
    }
}

// ---------------------------------------------------------------------------
// MessageBus
// ---------------------------------------------------------------------------

/// Capacity of the internal channels.
const BUS_CAPACITY: usize = 256;

#[derive(Debug)]
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Option<mpsc::Receiver<InboundMessage>>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
}

impl MessageBus {
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(BUS_CAPACITY);
        let (outbound_tx, _) = broadcast::channel(BUS_CAPACITY);
        Self {
            inbound_tx,
            inbound_rx: Some(inbound_rx),
            outbound_tx,
        }
    }

    /// Get a sender handle that channels use to push inbound messages.
    pub fn inbound_sender(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }

    /// Take the inbound receiver (can only be called once — the agent owns it).
    pub fn take_inbound_receiver(&mut self) -> Option<mpsc::Receiver<InboundMessage>> {
        self.inbound_rx.take()
    }

    /// Publish an outbound message (agent → channels).
    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        let _ = self.outbound_tx.send(msg);
    }

    /// Subscribe to outbound messages (each channel gets its own receiver).
    pub fn subscribe_outbound(&self) -> broadcast::Receiver<OutboundMessage> {
        self.outbound_tx.subscribe()
    }

    /// Clone the outbound sender (needed by AgentLoop).
    pub fn outbound_tx_clone(&self) -> broadcast::Sender<OutboundMessage> {
        self.outbound_tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::approval::{ApprovalRequest, ApprovalResponse};

    #[test]
    fn test_chat_message_serialization() {
        let msg = OutboundMessage::chat(
            "web".to_string(),
            "chat123".to_string(),
            "Hello, world!".to_string(),
            vec![],
            None,
        );

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"chat\""));
        assert!(json.contains("\"content\":\"Hello, world!\""));
        assert!(json.contains("\"channel\":\"web\""));
        assert!(json.contains("\"chat_id\":\"chat123\""));

        // 反序列化
        let deserialized: OutboundMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.channel, "web");
        assert_eq!(deserialized.chat_id, "chat123");
        match deserialized.message_type {
            OutboundMessageType::Chat { content, media } => {
                assert_eq!(content, "Hello, world!");
                assert_eq!(media.len(), 0);
            }
            _ => panic!("Expected Chat message type"),
        }
    }

    #[test]
    fn test_chat_message_with_media_serialization() {
        let msg = OutboundMessage::chat(
            "telegram".to_string(),
            "chat456".to_string(),
            "Check this out!".to_string(),
            vec!["image1.png".to_string(), "image2.jpg".to_string()],
            Some("msg123".to_string()),
        );

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"chat\""));
        assert!(json.contains("\"content\":\"Check this out!\""));
        assert!(json.contains("\"media\""));
        assert!(json.contains("image1.png"));
        assert!(json.contains("image2.jpg"));
        assert!(json.contains("\"reply_to\":\"msg123\""));

        // 反序列化
        let deserialized: OutboundMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.channel, "telegram");
        assert_eq!(deserialized.reply_to, Some("msg123".to_string()));
        match deserialized.message_type {
            OutboundMessageType::Chat { content, media } => {
                assert_eq!(content, "Check this out!");
                assert_eq!(media.len(), 2);
                assert_eq!(media[0], "image1.png");
                assert_eq!(media[1], "image2.jpg");
            }
            _ => panic!("Expected Chat message type"),
        }
    }

    #[test]
    fn test_approval_request_message_serialization() {
        let request = ApprovalRequest {
            id: "req123".to_string(),
            session_id: "session456".to_string(),
            channel: "web".to_string(),
            chat_id: "chat789".to_string(),
            command: "rm -rf /tmp/test".to_string(),
            working_dir: "/home/user".to_string(),
            context: "Test approval".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 300,
            display_message: None,
        };

        let msg = OutboundMessage::approval_request(
            "web".to_string(),
            "chat789".to_string(),
            request.clone(),
            None,
        );

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"approval_request\""));
        assert!(json.contains("\"request\""));
        assert!(json.contains("\"id\":\"req123\""));
        assert!(json.contains("\"command\":\"rm -rf /tmp/test\""));
        assert!(json.contains("\"session_id\":\"session456\""));

        // 反序列化
        let deserialized: OutboundMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.channel, "web");
        assert_eq!(deserialized.chat_id, "chat789");
        match deserialized.message_type {
            OutboundMessageType::ApprovalRequest { request: req } => {
                assert_eq!(req.id, "req123");
                assert_eq!(req.command, "rm -rf /tmp/test");
                assert_eq!(req.session_id, "session456");
                assert_eq!(req.working_dir, "/home/user");
                assert_eq!(req.timeout_secs, 300);
            }
            _ => panic!("Expected ApprovalRequest message type"),
        }
    }

    #[test]
    fn test_approval_request_message_struct_serialization() {
        let request = ApprovalRequest {
            id: "req456".to_string(),
            session_id: "session789".to_string(),
            channel: "telegram".to_string(),
            chat_id: "chat123".to_string(),
            command: "git push origin main".to_string(),
            working_dir: "/home/user/project".to_string(),
            context: "Push to production".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 600,
            display_message: None,
        };

        let msg = ApprovalRequestMessage {
            request: request.clone(),
        };

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"request\""));
        assert!(json.contains("\"id\":\"req456\""));
        assert!(json.contains("\"command\":\"git push origin main\""));

        // 反序列化
        let deserialized: ApprovalRequestMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.request.id, "req456");
        assert_eq!(deserialized.request.command, "git push origin main");
        assert_eq!(deserialized.request.session_id, "session789");
    }

    #[test]
    fn test_approval_response_message_struct_serialization() {
        let response = ApprovalResponse {
            request_id: "req789".to_string(),
            approved: true,
            responder: "user123".to_string(),
            timestamp: Utc::now(),
        };

        let msg = ApprovalResponseMessage {
            response: response.clone(),
        };

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"response\""));
        assert!(json.contains("\"request_id\":\"req789\""));
        assert!(json.contains("\"approved\":true"));
        assert!(json.contains("\"responder\":\"user123\""));

        // 反序列化
        let deserialized: ApprovalResponseMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.response.request_id, "req789");
        assert!(deserialized.response.approved);
        assert_eq!(deserialized.response.responder, "user123");
    }

    #[test]
    fn test_approval_response_rejected_serialization() {
        let response = ApprovalResponse {
            request_id: "req999".to_string(),
            approved: false,
            responder: "user456".to_string(),
            timestamp: Utc::now(),
        };

        let msg = ApprovalResponseMessage { response };

        // 序列化
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"approved\":false"));

        // 反序列化
        let deserialized: ApprovalResponseMessage = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.response.approved);
    }

    #[test]
    fn test_outbound_message_type_enum_serialization() {
        // 测试 Chat 类型
        let chat_type = OutboundMessageType::Chat {
            content: "Test".to_string(),
            media: vec![],
        };
        let json = serde_json::to_string(&chat_type).unwrap();
        assert!(json.contains("\"type\":\"chat\""));
        assert!(json.contains("\"content\":\"Test\""));

        // 测试 ApprovalRequest 类型
        let request = ApprovalRequest {
            id: "test".to_string(),
            session_id: "session".to_string(),
            channel: "web".to_string(),
            chat_id: "chat".to_string(),
            command: "test".to_string(),
            working_dir: "/".to_string(),
            context: "test".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 300,
            display_message: None,
        };
        let approval_type = OutboundMessageType::ApprovalRequest { request };
        let json = serde_json::to_string(&approval_type).unwrap();
        assert!(json.contains("\"type\":\"approval_request\""));
        assert!(json.contains("\"request\""));
    }
}
