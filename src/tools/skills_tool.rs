//! list_skills tool — delegates to the model to answer from the system prompt.

use anyhow::Result;
use serde_json::{json, Value};

use crate::tools::DynTool;

/// Tool that does not read the filesystem. The available skills (name + description) are already
/// in your system prompt under the "# Skills" section. Use that section to answer the user:
/// summarize, categorize, or list skills as appropriate.
pub struct ListSkillsTool;

impl ListSkillsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for ListSkillsTool {
    fn name(&self) -> &str {
        "list_skills"
    }

    fn description(&self) -> &str {
        "When the user asks what skills you have, which skills are available, or to list skills: your available skills and their descriptions are already in your system prompt under the '# Skills' section. Use that section to answer—summarize, categorize, or list them. Do not read the filesystem; answer from your system context."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        Ok("Your available skills and their descriptions are in your system prompt under '# Skills'. Use that section to answer the user: summarize, categorize, or list the skills as appropriate.".to_string())
    }
}
