//! Custom slash commands — prompt templates stored in the workspace.
//!
//! `<workspace>/.synbot/commands/<name>.md` defines `/name`. Optional frontmatter carries a
//! `description` (shown in the UI picker); the body is the prompt. `$ARGUMENTS` (or `{{args}}`) is
//! replaced with whatever the user typed after the command, and appended when the template has no
//! placeholder — so both "summarize $ARGUMENTS" and plain "review the diff" work.
//!
//! Expansion happens once, at the point a message enters the agent loop, so every channel gets the
//! same behaviour and the model never sees the slash syntax.

use serde::Serialize;
use std::path::Path;

/// Folder (relative to a workspace) holding command templates.
pub const COMMANDS_DIR: &str = ".synbot/commands";
/// Guard against a template swallowing the context window.
const MAX_BODY_CHARS: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Command {
    /// Command name without the leading slash (`review` for `/review`).
    pub name: String,
    pub description: String,
    pub body: String,
    /// Path relative to the workspace, for the UI to show where it came from.
    pub source: String,
}

/// Load every template in `<workspace>/.synbot/commands`, sorted by name.
pub fn load(workspace: &Path) -> Vec<Command> {
    let dir = workspace.join(COMMANDS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (description, body) = split_frontmatter(&raw);
        if body.trim().is_empty() {
            continue;
        }
        let mut body = body.trim_start_matches('\n').to_string();
        if body.chars().count() > MAX_BODY_CHARS {
            body = body.chars().take(MAX_BODY_CHARS).collect::<String>() + "\n...[truncated]";
        }
        out.push(Command {
            name: name.to_string(),
            description: description.unwrap_or_else(|| first_line(&body)),
            body,
            source: format!("{COMMANDS_DIR}/{name}.md"),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Expand `/name rest of the message` when `name` is a known template.
///
/// Returns `None` when the text is not a custom command, so the caller keeps the original message
/// (built-in commands such as `/stop` are handled elsewhere and must not be touched).
pub fn expand(workspace: &Path, text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('/')?;
    let name_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    let name = &rest[..name_end];
    if name.is_empty() {
        return None;
    }
    let commands = load(workspace);
    let command = commands.iter().find(|c| c.name == name)?;
    let args = rest[name_end..].trim();

    let has_placeholder = command.body.contains("$ARGUMENTS") || command.body.contains("{{args}}");
    let mut expanded = command
        .body
        .replace("$ARGUMENTS", args)
        .replace("{{args}}", args);
    if !has_placeholder && !args.is_empty() {
        expanded.push_str("\n\n");
        expanded.push_str(args);
    }
    Some(expanded)
}

/// `---\ndescription: x\n---\nbody` → (`Some("x")`, "body").
fn split_frontmatter(raw: &str) -> (Option<String>, String) {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (None, raw.to_string());
    }
    let after = &trimmed[3..];
    let Some(end) = after.find("\n---") else {
        return (None, raw.to_string());
    };
    let header = &after[..end];
    let body = after[end + 4..].to_string();
    let description = header.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim().eq_ignore_ascii_case("description") {
            Some(value.trim().trim_matches('"').to_string())
        } else {
            None
        }
    });
    (description.filter(|d| !d.is_empty()), body)
}

fn first_line(body: &str) -> String {
    let line = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let line = line.trim_start_matches('#').trim();
    if line.chars().count() > 80 {
        let mut cut: String = line.chars().take(80).collect();
        cut.push('…');
        cut
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cmd(dir: &Path, name: &str, content: &str) {
        let path = dir.join(COMMANDS_DIR);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{name}.md")), content).unwrap();
    }

    #[test]
    fn loads_frontmatter_and_falls_back_to_the_first_line() {
        let dir = tempfile::tempdir().unwrap();
        write_cmd(
            dir.path(),
            "review",
            "---\ndescription: Review a diff\n---\nReview $ARGUMENTS carefully.\n",
        );
        write_cmd(dir.path(), "triage", "# Triage the bug\n\nStep one.\n");

        let commands = load(dir.path());
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].name, "review");
        assert_eq!(commands[0].description, "Review a diff");
        assert_eq!(commands[0].source, ".synbot/commands/review.md");
        assert_eq!(commands[1].description, "Triage the bug");
    }

    #[test]
    fn expands_arguments_and_appends_when_missing_a_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        write_cmd(dir.path(), "review", "Review $ARGUMENTS carefully.");
        write_cmd(dir.path(), "standup", "Summarize yesterday.");

        let expanded = expand(dir.path(), "/review src/main.rs").unwrap();
        assert_eq!(expanded, "Review src/main.rs carefully.");

        let appended = expand(dir.path(), "/standup  and today").unwrap();
        assert_eq!(appended, "Summarize yesterday.\n\nand today");

        // Multi-line args survive; trailing spaces are trimmed.
        let multi = expand(dir.path(), "/review  a\nb  ").unwrap();
        assert_eq!(multi, "Review a\nb carefully.");
    }

    #[test]
    fn unknown_or_empty_commands_are_not_expanded() {
        let dir = tempfile::tempdir().unwrap();
        write_cmd(dir.path(), "review", "Review $ARGUMENTS.");
        assert!(expand(dir.path(), "/review").is_some());
        assert!(expand(dir.path(), "/stop").is_none());
        assert!(expand(dir.path(), "/").is_none());
        assert!(expand(dir.path(), "hello /review").is_none());
        assert!(expand(dir.path(), "").is_none());
    }

    #[test]
    fn empty_workspace_has_no_commands() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_empty());
        assert!(expand(dir.path(), "/anything").is_none());
    }

    #[test]
    fn oversized_templates_are_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let body = "x".repeat(MAX_BODY_CHARS + 100);
        write_cmd(dir.path(), "big", &body);
        let commands = load(dir.path());
        assert!(
            commands[0].body.ends_with("[truncated]"),
            "len {}",
            commands[0].body.len()
        );
    }
}
