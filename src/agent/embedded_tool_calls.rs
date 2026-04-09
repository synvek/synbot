//! Normalize tool invocations that models sometimes emit as plain text instead of native `tool_calls`.
//!
//! Supported shapes:
//! - `[TOOL_CALL]` … `[/TOOL_CALL]` (including Ruby-style `tool =>` / JSON inside).
//! - `[Tool: tool_name] {args}` or `[Tool: tool_name] ...` (same preview format as session persistence).
//!
//! Without this pass, the agent loop only executes [`AssistantContent::ToolCall`], so these never run.

use rig::completion::AssistantContent;
use serde_json::{json, Value};

/// Opening tag after `[` — matched case-insensitively (e.g. `TOOL_CALL` or `tool_call`).
const OPEN_SUFFIX: &[u8] = b"TOOL_CALL]";

fn find_open_tag(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'[' {
            continue;
        }
        let rest = &b[i + 1..];
        if rest.len() < OPEN_SUFFIX.len() {
            continue;
        }
        if rest[..OPEN_SUFFIX.len()].eq_ignore_ascii_case(OPEN_SUFFIX) {
            return Some(i);
        }
    }
    None
}

fn open_tag_len(s: &str, start: usize) -> usize {
    // `[` + TOOL_CALL + `]`  → find closing `]` of the open tag
    let after = &s[start + 1..];
    if let Some(end_rel) = after.find(']') {
        start + 1 + end_rel + 1 - start
    } else {
        1 + OPEN_SUFFIX.len()
    }
}

fn find_close_tag(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    // [/TOOL_CALL] — case-insensitive TOOL_CALL
    for i in 0..b.len().saturating_sub(3) {
        if b[i] != b'[' || b.get(i + 1) != Some(&b'/') {
            continue;
        }
        let rest = &b[i + 2..];
        if rest.len() < OPEN_SUFFIX.len() {
            continue;
        }
        if rest.len() >= OPEN_SUFFIX.len()
            && rest[..OPEN_SUFFIX.len()].eq_ignore_ascii_case(OPEN_SUFFIX)
        {
            return Some(i);
        }
    }
    None
}

fn close_tag_len(s: &str, start: usize) -> usize {
    let after = &s[start + 2..]; // after [/
    if let Some(end_rel) = after.find(']') {
        start + 2 + end_rel + 1 - start
    } else {
        2 + OPEN_SUFFIX.len()
    }
}

fn extract_balanced_json_object(s: &str) -> Option<&str> {
    let s = s.trim_start();
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `tool` / `tool` key with `=>` and a quoted name (Ruby-style).
fn extract_ruby_tool_name(body: &str) -> Option<String> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if !bytes[i..].starts_with(b"tool") {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if !before_ok {
            i += 1;
            continue;
        }
        // not a prefix of longer identifier
        if i + 4 < bytes.len() && bytes[i + 4].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let mut j = i + 4;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j + 1 < bytes.len() && bytes[j] == b'=' && bytes[j + 1] == b'>' {
            j += 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let rest = &body[j..];
            if let Some(stripped) = rest.strip_prefix('"') {
                let end = stripped.find('"')?;
                return Some(stripped[..end].to_string());
            }
            if let Some(stripped) = rest.strip_prefix('\'') {
                let end = stripped.find('\'')?;
                return Some(stripped[..end].to_string());
            }
        }
        i += 1;
    }
    None
}

fn extract_ruby_args_value(body: &str) -> Option<Value> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if !bytes[i..].starts_with(b"args") {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if !before_ok {
            i += 1;
            continue;
        }
        if i + 4 < bytes.len() && bytes[i + 4].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let mut j = i + 4;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j + 1 < bytes.len() && bytes[j] == b'=' && bytes[j + 1] == b'>' {
            j += 2;
            let tail = body[j..].trim_start();
            let json_str = extract_balanced_json_object(tail)?;
            return serde_json::from_str(json_str).ok();
        }
        i += 1;
    }
    None
}

fn parse_tool_call_block_body(body: &str) -> Option<(String, Value)> {
    let body = body.trim();
    if body.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(body) {
            let name = v
                .get("tool")
                .or_else(|| v.get("name"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            if let Some(name) = name {
                let args = v.get("args").cloned().unwrap_or(json!({}));
                return Some((name, args));
            }
        }
    }
    let name = extract_ruby_tool_name(body)?;
    let args = extract_ruby_args_value(body).unwrap_or(json!({}));
    Some((name, args))
}

struct SplitEmbedded {
    leading: String,
    calls: Vec<(String, Value)>,
    trailing: String,
}

/// One embedded invocation inside assistant text: byte range `[start, end)` in the original string.
struct EmbeddedInv {
    start: usize,
    end: usize,
    name: String,
    args: Value,
}

fn find_session_tool_marker(s: &str) -> Option<usize> {
    let pat = b"[tool:";
    let b = s.as_bytes();
    for i in 0..b.len().saturating_sub(pat.len()) {
        if b[i..i + pat.len()].eq_ignore_ascii_case(pat) {
            return Some(i);
        }
    }
    None
}

/// Parse `[Tool: name] optional_json_or_rest` starting at `start` (must point at `[`).
fn parse_session_style_invocation(s: &str, start: usize) -> Option<EmbeddedInv> {
    let pat = b"[tool:";
    let b = s.as_bytes();
    if start + pat.len() > b.len() || !b[start..start + pat.len()].eq_ignore_ascii_case(pat) {
        return None;
    }
    let name_start = start + pat.len();
    let after = &s[name_start..];
    let close_rel = after.find(']')?;
    let name = after[..close_rel].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let after_bracket = name_start + close_rel + 1;
    let raw_tail = &s[after_bracket..];
    let leading_ws = raw_tail.len() - raw_tail.trim_start().len();
    let tail = raw_tail.trim_start();
    let (args, consumed_in_tail) = if let Some(json_str) = extract_balanced_json_object(tail) {
        let v = serde_json::from_str(json_str).unwrap_or(json!({}));
        (v, leading_ws + json_str.len())
    } else {
        // No JSON object (e.g. "..." placeholder or empty) — drop rest of this line only.
        let line_take = tail
            .find('\n')
            .map(|n| leading_ws + n + 1)
            .unwrap_or_else(|| raw_tail.len());
        (json!({}), line_take)
    };
    Some(EmbeddedInv {
        start,
        end: after_bracket + consumed_in_tail,
        name,
        args,
    })
}

fn parse_wrapped_invocation_at(s: &str, start: usize) -> Option<EmbeddedInv> {
    if find_open_tag(&s[start..]) != Some(0) {
        return None;
    }
    let open_len = open_tag_len(s, start);
    let after_open = &s[start + open_len..];
    let close_start = find_close_tag(after_open)?;
    let body = after_open[..close_start].trim();
    let close_len = close_tag_len(after_open, close_start);
    let end = start + open_len + close_start + close_len;
    let (name, args) = parse_tool_call_block_body(body)?;
    Some(EmbeddedInv {
        start,
        end,
        name,
        args,
    })
}

fn find_next_embedded_invocation(rest: &str, base: usize) -> Option<EmbeddedInv> {
    let w = find_open_tag(rest).and_then(|rel| parse_wrapped_invocation_at(rest, rel));
    let s = find_session_tool_marker(rest).and_then(|rel| parse_session_style_invocation(rest, rel));
    let pick = match (w, s) {
        (Some(a), Some(b)) => {
            if a.start < b.start {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    Some(EmbeddedInv {
        start: base + pick.start,
        end: base + pick.end,
        name: pick.name,
        args: pick.args,
    })
}

fn split_embedded_tool_calls(text: &str) -> SplitEmbedded {
    let mut leading = String::new();
    let mut calls = Vec::new();
    let mut pos = 0usize;
    while pos < text.len() {
        let rest = &text[pos..];
        match find_next_embedded_invocation(rest, pos) {
            None => break,
            Some(inv) => {
                leading.push_str(&text[pos..inv.start]);
                calls.push((inv.name, inv.args));
                pos = inv.end;
            }
        }
    }
    SplitEmbedded {
        leading,
        calls,
        trailing: text[pos..].to_string(),
    }
}

/// Turn text-only `[TOOL_CALL]...[/TOOL_CALL]` segments into [`AssistantContent::tool_call`] items.
pub fn normalize_embedded_tool_calls(choice: Vec<AssistantContent>) -> Vec<AssistantContent> {
    let mut out = Vec::with_capacity(choice.len() + 2);
    for c in choice {
        match c {
            AssistantContent::Text(t) => {
                let split = split_embedded_tool_calls(&t.text);
                if split.calls.is_empty() {
                    out.push(AssistantContent::Text(t));
                    continue;
                }
                let lead = split.leading.trim();
                if !lead.is_empty() {
                    out.push(AssistantContent::text(lead.to_string()));
                }
                for (i, (name, args)) in split.calls.into_iter().enumerate() {
                    let id = format!("synbot_embedded_{}", i);
                    out.push(AssistantContent::tool_call(id, name, args));
                }
                let trail = split.trailing.trim();
                if !trail.is_empty() {
                    out.push(AssistantContent::text(trail.to_string()));
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ruby_style_list_cron_block() {
        let text = r#"[TOOL_CALL]
{tool => "list_cron_tasks", args => {

}}
[/TOOL_CALL]"#;
        let split = split_embedded_tool_calls(text);
        assert_eq!(split.calls.len(), 1);
        assert_eq!(split.calls[0].0, "list_cron_tasks");
        assert_eq!(split.calls[0].1, json!({}));
    }

    #[test]
    fn normalize_produces_tool_call_content() {
        let text = r#"[TOOL_CALL]
{tool => "list_cron_tasks", args => {}}
[/TOOL_CALL]"#;
        let out = normalize_embedded_tool_calls(vec![AssistantContent::text(text)]);
        assert_eq!(out.len(), 1);
        match &out[0] {
            AssistantContent::ToolCall(tc) => {
                assert_eq!(tc.function.name, "list_cron_tasks");
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn parses_session_style_tool_line() {
        let text = "[Tool: list_cron_tasks] ...";
        let split = split_embedded_tool_calls(text);
        assert_eq!(split.calls.len(), 1);
        assert_eq!(split.calls[0].0, "list_cron_tasks");
        assert_eq!(split.calls[0].1, json!({}));
        assert!(split.leading.is_empty());
        assert!(split.trailing.is_empty());
    }

    #[test]
    fn normalize_session_style_to_tool_call() {
        let out = normalize_embedded_tool_calls(vec![AssistantContent::text(
            "[Tool: list_cron_tasks] ...",
        )]);
        assert_eq!(out.len(), 1);
        match &out[0] {
            AssistantContent::ToolCall(tc) => assert_eq!(tc.function.name, "list_cron_tasks"),
            _ => panic!("expected ToolCall"),
        }
    }
}
