//! Control commands: /stop, /resume, /status, /clear (case-insensitive prefix).

/// Control command parsed from user message (trimmed content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlCommand {
    /// Stop current workflow or agent task.
    Stop,
    /// Resume workflow (same as /workflow continue).
    Resume,
    /// Show current session status and workflow state if any.
    Status,
    /// Clear session (same as reset_session tool).
    Clear,
}

const PREFIX_STOP: &str = "/stop";
const PREFIX_RESUME: &str = "/resume";
const PREFIX_STATUS: &str = "/status";
const PREFIX_CLEAR: &str = "/clear";

/// Returns true if content is exactly the command or command followed by optional whitespace only.
fn match_prefix(content: &str, prefix: &str) -> bool {
    let c = content.trim();
    c.eq_ignore_ascii_case(prefix)
        || (c.len() >= prefix.len()
            && c[..prefix.len()].eq_ignore_ascii_case(prefix)
            && c[prefix.len()..].trim().is_empty())
}

/// Parse control command. Only matches if the whole message is the command (or command + trailing space).
pub fn parse_control_command(content: &str) -> Option<ControlCommand> {
    let c = content.trim();
    if c.is_empty() {
        return None;
    }
    if match_prefix(c, PREFIX_STOP) {
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
    None
}

/// Hint text shown when agent/workflow is busy: list available control commands.
pub fn busy_hint_commands() -> &'static str {
    "Available commands: /stop (stop current work), /status (show session and workflow state), /clear (clear session), /resume (resume workflow)."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_matches() {
        assert_eq!(parse_control_command("/stop"), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("  /stop  "), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("/STOP"), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("/stop  "), Some(ControlCommand::Stop));
        assert_eq!(parse_control_command("/stop x"), None);
    }

    #[test]
    fn resume_status_clear() {
        assert_eq!(parse_control_command("/resume"), Some(ControlCommand::Resume));
        assert_eq!(parse_control_command("/status"), Some(ControlCommand::Status));
        assert_eq!(parse_control_command("/clear"), Some(ControlCommand::Clear));
    }

    #[test]
    fn non_control() {
        assert_eq!(parse_control_command("hello"), None);
        assert_eq!(parse_control_command("/workflow foo"), None);
    }
}
