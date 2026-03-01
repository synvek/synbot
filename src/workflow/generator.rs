//! Generate workflow definition (JSON) from user description via LLM.

use anyhow::{Context, Result};
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message};
use rig::OneOrMany;

use crate::rig_provider::SynbotCompletionModel;
use crate::workflow::types::WorkflowDef;

const SYSTEM_PROMPT: &str = r#"You are a workflow designer. Given a user's goal, output a single JSON object that defines a TurboWorkflow.

Output ONLY valid JSON, no markdown or explanation. The JSON must have this shape:
{
  "id": "optional-workflow-id",
  "name": "short name",
  "description": "what this workflow does",
  "inputs": [
    { "name": "param1", "description": "optional", "default": "optional default" }
  ],
  "steps": [
    {
      "id": "step1",
      "type": "llm",
      "description": "Task for the LLM to do (e.g. 'Draft an outline')"
    },
    {
      "id": "step2",
      "type": "user_input",
      "description": "Prompt shown to user (e.g. 'Please provide the title')",
      "input_key": "title"
    }
  ]
}

Rules:
- "steps" must be a non-empty array.
- Each step "type" is either "llm" or "user_input".
- "llm" steps: require "description", no "input_key".
- "user_input" steps: require "description" and "input_key" (key to store the user's reply).
- Break the user's goal into clear serial steps. Use user_input when human input is needed."#;

/// Generate a WorkflowDef from a natural language description using the given model.
pub async fn generate_workflow(
    model: &dyn SynbotCompletionModel,
    description: &str,
) -> Result<WorkflowDef> {
    let user_msg = format!(
        "Create a workflow (output only the JSON object, no other text):\n\n{}",
        description
    );
    let request = CompletionRequest {
        preamble: Some(SYSTEM_PROMPT.to_string()),
        chat_history: OneOrMany::one(Message::user(&user_msg)),
        tools: vec![],
        documents: vec![],
        temperature: Some(0.3),
        max_tokens: Some(4096),
        tool_choice: None,
        additional_params: None,
    };

    let response = model
        .completion(request)
        .await
        .map_err(|e| anyhow::anyhow!("workflow generation completion failed: {}", e))?;

    let mut text = String::new();
    for content in response.choice.clone().into_iter() {
        if let AssistantContent::Text(t) = content {
            text.push_str(&t.text);
        }
    }

    let text = text.trim();
    // Strip optional ```json ... ``` wrapper
    let json_str = if text.starts_with("```") {
        let after = text[3..].trim_start();
        let after = if after.to_lowercase().starts_with("json") {
            after[4..].trim_start()
        } else {
            after
        };
        after.strip_suffix("```").unwrap_or(after).trim()
    } else {
        text
    };

    let def: WorkflowDef = serde_json::from_str(json_str)
        .context("parse workflow JSON from model response")?;
    def.validate()
        .map_err(|e| anyhow::anyhow!("invalid workflow: {}", e))?;
    Ok(def)
}
