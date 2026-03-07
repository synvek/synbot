//! Diff viewer tool: show unified diff between original content and current file content.
//!
//! Registered as `show_diff` in the tool registry. Reads the current file,
//! compares with the provided original content, and produces unified diff output.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::info;

use crate::tools::context;
use crate::tools::DynTool;

pub struct DiffViewerTool {
    pub workspace: PathBuf,
    pub restrict: bool,
    pub max_diff_lines: usize,
}

// ---- path resolution (same pattern as filesystem.rs) ----

#[cfg(windows)]
fn path_for_prefix_check(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let s = s.trim_start_matches(r"\\?\");
    PathBuf::from(s)
}

#[cfg(not(windows))]
fn path_for_prefix_check(p: &Path) -> PathBuf {
    p.to_path_buf()
}

fn resolve_path(path: &str, workspace: &Path, restrict: bool) -> anyhow::Result<PathBuf> {
    let effective_workspace = context::current_allowed_roots()
        .unwrap_or_else(|| workspace.to_path_buf());
    let p = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        effective_workspace.join(path)
    };
    let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
    let canonical_norm = path_for_prefix_check(&canonical);

    let allowed = if let Some(ctx_workspace) = context::current_allowed_roots() {
        let ws_canon = ctx_workspace.canonicalize().unwrap_or_else(|_| ctx_workspace.clone());
        let ws_norm = path_for_prefix_check(&ws_canon);
        canonical_norm.starts_with(&ws_norm)
    } else if restrict {
        let ws_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        let ws_norm = path_for_prefix_check(&ws_canon);
        canonical_norm.starts_with(&ws_norm)
    } else {
        true
    };
    if !allowed {
        anyhow::bail!(
            "Path {} is outside current agent scope (allowed: this agent's workspace only)",
            path
        );
    }
    Ok(p)
}

// ---- unified diff generation ----

/// A single diff hunk with context.
struct Hunk {
    old_start: usize, // 1-based
    old_count: usize,
    new_start: usize, // 1-based
    new_count: usize,
    lines: Vec<String>,
}

/// Compute the longest common subsequence table for two slices of lines.
fn lcs_table(old: &[&str], new: &[&str]) -> Vec<Vec<usize>> {
    let m = old.len();
    let n = new.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    dp
}

/// Produce a list of edit operations from the LCS table.
/// Each entry is: ('=', old_idx, new_idx), ('-', old_idx, _), ('+', _, new_idx)
fn diff_ops(old: &[&str], new: &[&str]) -> Vec<(char, usize, usize)> {
    let dp = lcs_table(old, new);
    let mut ops = Vec::new();
    let (mut i, mut j) = (old.len(), new.len());
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            ops.push(('=', i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(('+', 0, j - 1));
            j -= 1;
        } else {
            ops.push(('-', i - 1, 0));
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

/// Generate unified diff string from original and modified content.
/// Uses 3 lines of context around each change, merging overlapping hunks.
pub fn generate_unified_diff(path: &str, original: &str, modified: &str) -> String {
    let old_lines: Vec<&str> = original.lines().collect();
    let new_lines: Vec<&str> = modified.lines().collect();

    let ops = diff_ops(&old_lines, &new_lines);

    // Mark which ops are changes (not '=')
    let change_indices: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (op, _, _))| *op != '=')
        .map(|(i, _)| i)
        .collect();

    if change_indices.is_empty() {
        return String::new();
    }

    let context = 3usize;

    // Build hunks: group changes that are within `context` lines of each other
    let mut hunks: Vec<Hunk> = Vec::new();

    // We need to track old/new line positions for each op
    let mut positions: Vec<(usize, usize)> = Vec::with_capacity(ops.len());
    {
        let (mut oi, mut ni) = (0usize, 0usize);
        for &(op, _, _) in &ops {
            positions.push((oi, ni));
            match op {
                '=' => { oi += 1; ni += 1; }
                '-' => { oi += 1; }
                '+' => { ni += 1; }
                _ => {}
            }
        }
    }

    // Group changes into hunk ranges (indices into ops)
    let mut groups: Vec<(usize, usize)> = Vec::new();
    {
        let mut group_start = change_indices[0];
        let mut group_end = change_indices[0];
        for &ci in &change_indices[1..] {
            // Check if this change is within context distance of the previous
            if ci - group_end <= context * 2 {
                group_end = ci;
            } else {
                groups.push((group_start, group_end));
                group_start = ci;
                group_end = ci;
            }
        }
        groups.push((group_start, group_end));
    }

    // Build each hunk
    for (group_start, group_end) in groups {
        let ctx_start = if group_start > context { group_start - context } else { 0 };
        let ctx_end = (group_end + context + 1).min(ops.len());

        let old_start = positions[ctx_start].0 + 1; // 1-based
        let new_start = positions[ctx_start].1 + 1;

        let mut old_count = 0usize;
        let mut new_count = 0usize;
        let mut lines = Vec::new();

        for idx in ctx_start..ctx_end {
            let (op, _, _) = ops[idx];
            match op {
                '=' => {
                    let line = old_lines[positions[idx].0];
                    lines.push(format!(" {}", line));
                    old_count += 1;
                    new_count += 1;
                }
                '-' => {
                    let line = old_lines[positions[idx].0];
                    lines.push(format!("-{}", line));
                    old_count += 1;
                }
                '+' => {
                    let line = new_lines[positions[idx].1];
                    lines.push(format!("+{}", line));
                    new_count += 1;
                }
                _ => {}
            }
        }

        hunks.push(Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines,
        });
    }

    // Format output
    let mut output = String::new();
    output.push_str(&format!("--- a/{}\n", path));
    output.push_str(&format!("+++ b/{}\n", path));

    for hunk in &hunks {
        output.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        ));
        for line in &hunk.lines {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

// ---- DynTool implementation ----

#[async_trait::async_trait]
impl DynTool for DiffViewerTool {
    fn name(&self) -> &str {
        "show_diff"
    }

    fn description(&self) -> &str {
        "Show unified diff between original content and current file content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path (relative to workspace)"
                },
                "original_content": {
                    "type": "string",
                    "description": "Original file content before modification"
                }
            },
            "required": ["path", "original_content"]
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'path' is required and must be a string"))?;
        let original_content = args["original_content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'original_content' is required and must be a string"))?;

        let resolved = resolve_path(path_str, &self.workspace, self.restrict)?;
        info!(path = %resolved.display(), "show_diff");

        let current_content = tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| anyhow::anyhow!("Cannot read file '{}': {}", path_str, e))?;

        let diff = generate_unified_diff(path_str, original_content, &current_content);

        if diff.is_empty() {
            return Ok("No differences found.".to_string());
        }

        // Count diff lines and truncate if needed
        let diff_lines: Vec<&str> = diff.lines().collect();
        let total = diff_lines.len();
        if total > self.max_diff_lines {
            let truncated: String = diff_lines[..self.max_diff_lines].join("\n");
            Ok(format!(
                "{}\n... (truncated, {} total diff lines)",
                truncated, total
            ))
        } else {
            Ok(diff)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_differences() {
        let content = "line1\nline2\nline3\n";
        let diff = generate_unified_diff("test.rs", content, content);
        assert!(diff.is_empty());
    }

    #[test]
    fn test_simple_addition() {
        let original = "line1\nline2\nline3\n";
        let modified = "line1\nline2\nnew_line\nline3\n";
        let diff = generate_unified_diff("test.rs", original, modified);
        assert!(diff.contains("--- a/test.rs"));
        assert!(diff.contains("+++ b/test.rs"));
        assert!(diff.contains("@@"));
        assert!(diff.contains("+new_line"));
    }

    #[test]
    fn test_simple_deletion() {
        let original = "line1\nline2\nline3\n";
        let modified = "line1\nline3\n";
        let diff = generate_unified_diff("test.rs", original, modified);
        assert!(diff.contains("-line2"));
    }

    #[test]
    fn test_simple_modification() {
        let original = "line1\nline2\nline3\n";
        let modified = "line1\nmodified\nline3\n";
        let diff = generate_unified_diff("test.rs", original, modified);
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_diff_header_format() {
        let original = "a\nb\nc\n";
        let modified = "a\nx\nc\n";
        let diff = generate_unified_diff("src/main.rs", original, modified);
        assert!(diff.starts_with("--- a/src/main.rs\n+++ b/src/main.rs\n"));
    }

    #[test]
    fn test_hunk_header_format() {
        let original = "a\nb\nc\n";
        let modified = "a\nx\nc\n";
        let diff = generate_unified_diff("test.rs", original, modified);
        // Should contain @@ markers with line numbers
        let hunk_line = diff.lines().find(|l| l.starts_with("@@")).unwrap();
        assert!(hunk_line.contains("@@"));
        // Verify format: @@ -start,count +start,count @@
        assert!(hunk_line.starts_with("@@ -"));
        assert!(hunk_line.ends_with(" @@"));
    }

    #[test]
    fn test_empty_original() {
        let diff = generate_unified_diff("test.rs", "", "new content\n");
        assert!(diff.contains("+new content"));
    }

    #[test]
    fn test_empty_modified() {
        let diff = generate_unified_diff("test.rs", "old content\n", "");
        assert!(diff.contains("-old content"));
    }

    #[test]
    fn test_both_empty() {
        let diff = generate_unified_diff("test.rs", "", "");
        assert!(diff.is_empty());
    }

    #[test]
    fn test_tool_name() {
        let tool = DiffViewerTool {
            workspace: PathBuf::from("/tmp"),
            restrict: true,
            max_diff_lines: 500,
        };
        assert_eq!(tool.name(), "show_diff");
    }

    #[test]
    fn test_tool_description() {
        let tool = DiffViewerTool {
            workspace: PathBuf::from("/tmp"),
            restrict: true,
            max_diff_lines: 500,
        };
        assert_eq!(
            tool.description(),
            "Show unified diff between original content and current file content."
        );
    }

    #[test]
    fn test_parameters_schema_has_required_fields() {
        let tool = DiffViewerTool {
            workspace: PathBuf::from("/tmp"),
            restrict: true,
            max_diff_lines: 500,
        };
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("path"));
        assert!(props.contains_key("original_content"));
        let required = schema["required"].as_array().unwrap();
        let req_strs: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(req_strs.contains(&"path"));
        assert!(req_strs.contains(&"original_content"));
    }

    #[tokio::test]
    async fn test_call_file_not_found() {
        let tool = DiffViewerTool {
            workspace: PathBuf::from("/tmp"),
            restrict: false,
            max_diff_lines: 500,
        };
        let args = json!({
            "path": "/tmp/__nonexistent_file_for_test__.txt",
            "original_content": "hello"
        });
        let result = tool.call(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Cannot read file"));
    }

    #[tokio::test]
    async fn test_call_no_diff() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello\nworld\n").unwrap();

        let tool = DiffViewerTool {
            workspace: dir.path().to_path_buf(),
            restrict: false,
            max_diff_lines: 500,
        };
        let args = json!({
            "path": file_path.to_str().unwrap(),
            "original_content": "hello\nworld\n"
        });
        let result = tool.call(args).await.unwrap();
        assert_eq!(result, "No differences found.");
    }

    #[tokio::test]
    async fn test_call_with_diff() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello\nworld\n").unwrap();

        let tool = DiffViewerTool {
            workspace: dir.path().to_path_buf(),
            restrict: false,
            max_diff_lines: 500,
        };
        let args = json!({
            "path": file_path.to_str().unwrap(),
            "original_content": "hello\nold\n"
        });
        let result = tool.call(args).await.unwrap();
        assert!(result.contains("--- a/"));
        assert!(result.contains("+++ b/"));
        assert!(result.contains("-old"));
        assert!(result.contains("+world"));
    }

    #[tokio::test]
    async fn test_call_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("big.txt");
        // Create a file with many different lines to produce a large diff
        let modified: String = (0..600).map(|i| format!("new_line_{}\n", i)).collect();
        std::fs::write(&file_path, &modified).unwrap();

        let original: String = (0..600).map(|i| format!("old_line_{}\n", i)).collect();

        let tool = DiffViewerTool {
            workspace: dir.path().to_path_buf(),
            restrict: false,
            max_diff_lines: 10, // very small limit to trigger truncation
        };
        let args = json!({
            "path": file_path.to_str().unwrap(),
            "original_content": original
        });
        let result = tool.call(args).await.unwrap();
        assert!(result.contains("... (truncated,"));
        assert!(result.contains("total diff lines)"));
    }

    #[tokio::test]
    async fn test_call_path_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let tool = DiffViewerTool {
            workspace: dir.path().to_path_buf(),
            restrict: true,
            max_diff_lines: 500,
        };
        let args = json!({
            "path": "/etc/passwd",
            "original_content": ""
        });
        let result = tool.call(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("outside current agent scope"));
    }

    #[tokio::test]
    async fn test_call_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("sub").join("test.txt");
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(&file_path, "content\n").unwrap();

        let tool = DiffViewerTool {
            workspace: dir.path().to_path_buf(),
            restrict: false,
            max_diff_lines: 500,
        };
        let args = json!({
            "path": "sub/test.txt",
            "original_content": "old\n"
        });
        let result = tool.call(args).await.unwrap();
        assert!(result.contains("-old"));
        assert!(result.contains("+content"));
    }
}
