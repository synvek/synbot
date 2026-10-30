//! Control commands: /stop, /resume, /status, /clear, /commands, /skills, /tools,
//! /pending, /approve, /reject, /tasks (case-insensitive prefix).

/// Control command parsed from user message (trimmed content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlCommand {
    /// Stop current workflow or agent task. /cancel is an alias for /stop.
    Stop,
    /// Resume workflow (same as /workflow continue).
    Resume,
    /// Show current session status and workflow state if any.
    Status,
    /// Clear session (same as reset_session tool).
    Clear,
    /// List available slash commands (help).
    Commands,
    /// List skills available to this instance (filesystem + plugins), same source as the system prompt # Skills section.
    Skills,
    /// List tools currently registered for the agent (name + short description).
    Tools,
    /// List approval requests waiting for an answer in this chat.
    Pending,
    /// Answer a waiting approval from plain text (no buttons): `/approve [request-id-prefix]`.
    Approve { target: Option<String> },
    /// Reject a waiting approval: `/reject [request-id-prefix]`.
    Reject { target: Option<String> },
    /// Show the task board of the current project.
    Tasks,
}

const PREFIX_STOP: &str = "/stop";
const PREFIX_CANCEL: &str = "/cancel";
const PREFIX_RESUME: &str = "/resume";
const PREFIX_STATUS: &str = "/status";
const PREFIX_CLEAR: &str = "/clear";
const PREFIX_COMMANDS: &str = "/commands";
const PREFIX_HELP: &str = "/help";
const PREFIX_SKILLS: &str = "/skills";
const PREFIX_TOOLS: &str = "/tools";
const PREFIX_PENDING: &str = "/pending";
const PREFIX_APPROVALS: &str = "/approvals";
const PREFIX_APPROVE: &str = "/approve";
const PREFIX_REJECT: &str = "/reject";
const PREFIX_TASKS: &str = "/tasks";
const PREFIX_TODO: &str = "/todo";

/// Returns true if content is exactly the command or command followed by optional whitespace only.
/// Uses get() for slicing so we never split in the middle of a multi-byte UTF-8 character.
fn match_prefix(content: &str, prefix: &str) -> bool {
    let c = content.trim();
    if c.eq_ignore_ascii_case(prefix) {
        return true;
    }
    let Some(head) = c.get(..prefix.len()) else {
        return false;
    };
    head.eq_ignore_ascii_case(prefix) && c[prefix.len()..].trim().is_empty()
}

/// Returns the trailing argument when `content` starts with `prefix` on a word boundary.
fn match_prefix_arg<'a>(content: &'a str, prefix: &str) -> Option<&'a str> {
    let c = content.trim();
    let head = c.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = &c[prefix.len()..];
    if rest.is_empty() {
        return Some("");
    }
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// A request id may only be matched by a hex prefix of a UUID: anything else is ignored rather
/// than forwarded to the agent as free text.
fn parse_request_target(raw: &str) -> Option<String> {
    let token = raw.split_whitespace().next().unwrap_or("");
    if token.is_empty() || token.len() > 36 {
        return None;
    }
    token
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == '-')
        .then(|| token.to_ascii_lowercase())
}

/// `/approve` may be called with no argument (the only waiting request) or with an id prefix.
/// Anything else is left to the agent instead of answering a request the user did not name.
fn optional_request_target(raw: &str) -> Option<Option<String>> {
    if raw.trim().is_empty() {
        return Some(None);
    }
    parse_request_target(raw).map(Some)
}

/// Parse control command. Only matches if the whole message is the command (or command + trailing space).
pub fn parse_control_command(content: &str) -> Option<ControlCommand> {
    let c = content.trim();
    if c.is_empty() {
        return None;
    }
    if match_prefix(c, PREFIX_STOP) || match_prefix(c, PREFIX_CANCEL) {
        return Some(ControlCommand::Stop);
    }
    if match_prefix(c, PREFIX_RESUME) {
        return Some(ControlCommand::Resume);
    }
    if match_prefix(c, PREFIX_STATUS) {
        return Some(ControlCommand::Status);
    }
    if match_prefix(c, PREFIX_CLEAR) {
        return Some(ControlCommand::Clear);
    }
    if match_prefix(c, PREFIX_COMMANDS) || match_prefix(c, PREFIX_HELP) {
        return Some(ControlCommand::Commands);
    }
    if match_prefix(c, PREFIX_SKILLS) {
        return Some(ControlCommand::Skills);
    }
    if match_prefix(c, PREFIX_TOOLS) {
        return Some(ControlCommand::Tools);
    }
    if match_prefix(c, PREFIX_PENDING) || match_prefix(c, PREFIX_APPROVALS) {
        return Some(ControlCommand::Pending);
    }
    if match_prefix(c, PREFIX_TASKS) || match_prefix(c, PREFIX_TODO) {
        return Some(ControlCommand::Tasks);
    }
    if let Some(rest) = match_prefix_arg(c, PREFIX_APPROVE) {
        if let Some(target) = optional_request_target(rest) {
            return Some(ControlCommand::Approve { target });
        }
    }
    if let Some(rest) = match_prefix_arg(c, PREFIX_REJECT) {
        if let Some(target) = optional_request_target(rest) {
            return Some(ControlCommand::Reject { target });
        }
    }
    None
}

/// Hint text shown when agent/workflow is busy: list available control commands.
pub fn busy_hint_commands() -> &'static str {
    "Available commands: /commands (list commands), /skills (list skills), /tools (list tools), /stop or /cancel (stop current work), /status (show session and workflow state), /pending (approvals waiting), /approve or /reject [id-prefix] (answer one), /tasks (task board), /clear (clear session), /resume (resume workflow)."
}

/// User-facing help text for slash commands.
pub fn slash_commands_help_text() -> &'static str {
    "Slash commands (case-insensitive; command must be alone or with trailing spaces):\n\
\n\
- /workflow <description>: create and run a workflow from a task description\n\
- /workflow continue: continue the current session's saved workflow\n\
- /workflow + JSON: create a workflow from a JSON definition (bot will confirm)\n\
- /resume: resume workflow (same as /workflow continue)\n\
- /stop or /cancel: stop the current running workflow/agent task\n\
- /status: show current session info and workflow state\n\
- /pending (or /approvals): list approval requests waiting for an answer\n\
- /approve [id-prefix] or /reject [id-prefix]: answer a waiting approval without buttons\n\
- /tasks (or /todo): show the task board of the current project\n\
- /clear: clear the current session (history + workflow state)\n\
- /commands (or /help): show this list\n\
- /skills: list available skills (SKILL.md under the skills directory + plugin skills)\n\
- /tools: list available tools (name and short description)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_matches() {
        assert_eq!(parse_control_command("/stop"), Some(ControlCommand::Stop));
        assert_eq!(
            parse_control_command("  /stop  "),
            Some(ControlCommand::Stop)
        );
        assert_eq!(parse_control_command("/STOP"), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("/stop  "), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("/stop x"), None);
    }

    #[test]
    fn cancel_is_alias_for_stop() {
        assert_eq!(parse_control_command("/cancel"), Some(ControlCommand::Stop));
        assert_eq!(
            parse_control_command("  /cancel  "),
            Some(ControlCommand::Stop)
        );
        assert_eq!(parse_control_command("/CANCEL"), Some(ControlCommand::Stop));
        assert_eq!(
            parse_control_command("/cancel  "),
            Some(ControlCommand::Stop)
        );
        assert_eq!(parse_control_command("/cancel x"), None);
    }

    #[test]
    fn resume_status_clear() {
        assert_eq!(
            parse_control_command("/resume"),
            Some(ControlCommand::Resume)
        );
        assert_eq!(
            parse_control_command("/status"),
            Some(ControlCommand::Status)
        );
        assert_eq!(parse_control_command("/clear"), Some(ControlCommand::Clear));
    }

    #[test]
    fn commands_and_help() {
        assert_eq!(
            parse_control_command("/commands"),
            Some(ControlCommand::Commands)
        );
        assert_eq!(
            parse_control_command("/help"),
            Some(ControlCommand::Commands)
        );
        assert_eq!(
            parse_control_command("  /commands  "),
            Some(ControlCommand::Commands)
        );
        assert_eq!(
            parse_control_command("/help  "),
            Some(ControlCommand::Commands)
        );
        assert_eq!(parse_control_command("/commands now"), None);
    }

    #[test]
    fn skills_and_tools() {
        assert_eq!(
            parse_control_command("/skills"),
            Some(ControlCommand::Skills)
        );
        assert_eq!(
            parse_control_command("  /skills  "),
            Some(ControlCommand::Skills)
        );
        assert_eq!(
            parse_control_command("/SKILLS"),
            Some(ControlCommand::Skills)
        );
        assert_eq!(parse_control_command("/skills extra"), None);
        assert_eq!(parse_control_command("/tools"), Some(ControlCommand::Tools));
        assert_eq!(
            parse_control_command("/TOOLS "),
            Some(ControlCommand::Tools)
        );
        assert_eq!(parse_control_command("/tools x"), None);
    }

    #[test]
    fn pending_and_tasks() {
        assert_eq!(
            parse_control_command("/pending"),
            Some(ControlCommand::Pending)
        );
        assert_eq!(
            parse_control_command("  /APPROVALS  "),
            Some(ControlCommand::Pending)
        );
        assert_eq!(parse_control_command("/pending now"), None);
        assert_eq!(parse_control_command("/tasks"), Some(ControlCommand::Tasks));
        assert_eq!(parse_control_command("/TODO"), Some(ControlCommand::Tasks));
        assert_eq!(parse_control_command("/tasks x"), None);
    }

    #[test]
    fn approve_and_reject_parse_optional_target() {
        assert_eq!(
            parse_control_command("/approve"),
            Some(ControlCommand::Approve { target: None })
        );
        assert_eq!(
            parse_control_command("  /reject  "),
            Some(ControlCommand::Reject { target: None })
        );
        assert_eq!(
            parse_control_command("/approve 3f2a1b"),
            Some(ControlCommand::Approve {
                target: Some("3f2a1b".to_string())
            })
        );
        assert_eq!(
            parse_control_command("/REJECT 3F2A1B extra words"),
            Some(ControlCommand::Reject {
                target: Some("3f2a1b".to_string())
            })
        );
        // A non hex argument is not treated as an approval command at all.
        assert_eq!(parse_control_command("/approve please"), None);
        assert_eq!(parse_control_command("/approve not-a-uuid"), None);
        assert_eq!(parse_control_command("/approve /etc/passwd"), None);
    }

    #[test]
    fn non_control() {
        assert_eq!(parse_control_command("hello"), None);
        assert_eq!(parse_control_command("/workflow foo"), None);
    }

    #[test]
    fn non_ascii_content_does_not_panic() {
        // Byte index 5 would split the middle of '成' (UTF-8 bytes 3..6). Must not panic.
        assert_eq!(parse_control_command("生成一张照片，一只小猫在看书"), None);
        assert_eq!(parse_control_command("日本語"), None);
    }
}
