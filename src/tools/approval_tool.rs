//! Tool for the agent to submit an approval response (interpreted from user message).

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::sync::Arc;

use crate::tools::approval::{ApprovalManager, ApprovalResponse};
use crate::tools::DynTool;

#[derive(Clone)]
pub struct SubmitApprovalResponseTool {
    pub approval_manager: Arc<ApprovalManager>,
}

#[async_trait::async_trait]
impl DynTool for SubmitApprovalResponseTool {
    fn name(&self) -> &str {
        "submit_approval_response"
    }

    fn description(&self) -> &str {
        "Submit your interpretation of the user's response to a pending command approval request. Call this when the message metadata indicates pending_approval_request_id and the user has replied (in any language). Use approved=true for agree/yes/approve, approved=false for reject/no/deny."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "request_id": { "type": "string", "description": "The pending approval request ID from metadata" },
                "approved": { "type": "boolean", "description": "True if the user approved, false if they rejected" }
            },
            "required": ["request_id", "approved"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let request_id = args
            .get("request_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("request_id required"))?
            .to_string();
        let approved = args
            .get("approved")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("approved required"))?;
        let responder = args
            .get("responder")
            .and_then(|v| v.as_str())
            .unwrap_or("agent")
            .to_string();

        let response = ApprovalResponse {
            request_id,
            approved,
            responder,
            timestamp: Utc::now(),
        };
        self.approval_manager.submit_response(response).await?;
        Ok(if approved { "approved" } else { "rejected" }.to_string())
    }
}
