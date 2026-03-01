//! Workflow trigger: only prefix `/workflow` (case-insensitive). No intent detection.

use crate::workflow::types::WorkflowDef;

const PREFIX: &str = "/workflow";
const PREFIX_LEN: usize = 9; // "/workflow".len()

/// Result of parsing user message for workflow.
#[derive(Debug, Clone)]
pub enum WorkflowTrigger {
    /// Create and run a new workflow. Body after prefix is description and/or JSON.
    Create {
        description: String,
        user_provided_def: Option<WorkflowDef>,
    },
    /// Continue the current session's persisted workflow.
    Continue,
    /// Not a workflow message.
    None,
}

/// Try to extract a JSON object from content: ```json ... ``` or first {...}.
fn extract_workflow_json(content: &str) -> Option<WorkflowDef> {
    let content = content.trim();
    if let Some(start) = content.find("```") {
        let after = content[start + 3..].trim_start();
        let block: &str = if after.to_lowercase().starts_with("json") {
            after[4.min(after.len())..].trim_start()
        } else {
            after
        };
        if let Some(end) = block.find("```") {
            let json_str = block[..end].trim();
            if let Ok(def) = serde_json::from_str::<WorkflowDef>(json_str) {
                return Some(def);
            }
        }
    }
    if let Some(start) = content.find('{') {
        let mut depth = 0u32;
        let bytes = content.as_bytes();
        for (i, &b) in bytes.iter().enumerate().skip(start) {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let json_str = &content[start..=i];
                        if let Ok(def) = serde_json::from_str::<WorkflowDef>(json_str) {
                            return Some(def);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Remove first ```...``` or {...} block from content; return (rest as description, parsed def if any).
fn strip_json_from_content(content: &str) -> (String, Option<WorkflowDef>) {
    let def = extract_workflow_json(content);
    let s = content.trim();
    let without = if def.is_some() {
        if let Some(back_start) = s.find("```") {
            let after = s[back_start + 3..].trim_start();
            let skip = if after.to_lowercase().starts_with("json") { 4 } else { 0 };
            let after = after[skip..].trim_start();
            if let Some(back_end) = after.find("```") {
                let end_pos = back_start + 3 + (if skip == 4 { 4 } else { 0 }) + back_end + 3;
                format!("{} {}", s[..back_start].trim(), s.get(end_pos..).unwrap_or("").trim())
                    .trim()
                    .to_string()
            } else {
                s.to_string()
            }
        } else if let Some(start) = s.find('{') {
            let mut depth = 0u32;
            let bytes = s.as_bytes();
            let mut end = start;
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                match b {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            format!(
                "{} {}",
                s[..start].trim(),
                s.get(end + 1..).unwrap_or("").trim()
            )
            .trim()
            .to_string()
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };
    let desc = without
        .trim()
        .trim_start_matches(|c: char| c == ' ' || c == '\n' || c == ':')
        .to_string();
    (desc, def)
}

/// Parse user message: only prefix `/workflow` (case-insensitive). Body after prefix:
/// - "continue" (case-insensitive) → Continue
/// - else → Create with body as description (and optional JSON).
pub fn parse_workflow_trigger(content: &str) -> WorkflowTrigger {
    let content = content.trim();
    if content.is_empty() {
        return WorkflowTrigger::None;
    }

    let lower = content.to_lowercase();
    if !lower.starts_with(PREFIX) {
        return WorkflowTrigger::None;
    }

    let body = content[PREFIX_LEN..].trim_start_matches(|c: char| c == ' ' || c == ':' || c == '\n');
    if body.is_empty() {
        return WorkflowTrigger::Create {
            description: String::new(),
            user_provided_def: None,
        };
    }

    if body.eq_ignore_ascii_case("continue") {
        return WorkflowTrigger::Continue;
    }

    let (description, user_provided_def) = strip_json_from_content(body);
    WorkflowTrigger::Create {
        description: description.trim().to_string(),
        user_provided_def,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_workflow_create() {
        let r = parse_workflow_trigger("/workflow generate a doc outline");
        match &r {
            WorkflowTrigger::Create { description, .. } => {
                assert!(description.contains("generate") || description.contains("outline"));
            }
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn prefix_workflow_case_insensitive() {
        let r = parse_workflow_trigger("/Workflow do something");
        match &r {
            WorkflowTrigger::Create { description, .. } => assert!(description.contains("something")),
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn prefix_workflow_continue() {
        assert!(matches!(
            parse_workflow_trigger("/workflow continue"),
            WorkflowTrigger::Continue
        ));
        assert!(matches!(
            parse_workflow_trigger("/workflow   continue"),
            WorkflowTrigger::Continue
        ));
        assert!(matches!(
            parse_workflow_trigger("/WORKFLOW CONTINUE"),
            WorkflowTrigger::Continue
        ));
    }

    #[test]
    fn not_trigger() {
        assert!(matches!(
            parse_workflow_trigger("hello world"),
            WorkflowTrigger::None
        ));
        assert!(matches!(
            parse_workflow_trigger("twfw something"),
            WorkflowTrigger::None
        ));
        assert!(matches!(
            parse_workflow_trigger("create workflow"),
            WorkflowTrigger::None
        ));
    }
}
