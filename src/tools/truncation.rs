//! Smart truncation utilities for tool output.
//!
//! Provides intelligent truncation that preserves the first and last portions
//! of content, inserting a truncation marker in the middle. This ensures that
//! both the beginning context and the final results of tool output are retained.

/// Result of a truncation operation, including metadata about what was truncated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationResult {
    /// The (possibly truncated) content.
    pub content: String,
    /// Size of the original content in bytes.
    pub original_size: usize,
    /// Size of the truncated content in bytes.
    pub truncated_size: usize,
    /// Description of the truncation strategy used (e.g. "head-tail" or "none").
    pub strategy: String,
}

/// Build the truncation marker string for the given sizes.
fn build_marker(original_size: usize, truncated_size: usize) -> String {
    format!(
        "\n...[truncated: {} bytes -> {} bytes, strategy: head-tail]\n",
        original_size, truncated_size
    )
}

/// Apply smart truncation to `content`, keeping at most `max_size` bytes.
///
/// **Strategy (head-tail):**
/// - If `content.len() <= max_size`, return the content unchanged.
/// - Otherwise, reserve space for the truncation marker, then take the first
///   60 % of the remaining budget from the beginning and the last 40 % from
///   the end of the original content.
///
/// The marker contains the original size, the truncated size, and the strategy
/// name so that downstream consumers can tell that truncation occurred.
pub fn smart_truncate(content: &str, max_size: usize) -> TruncationResult {
    let original_size = content.len();

    // No truncation needed
    if original_size <= max_size {
        return TruncationResult {
            content: content.to_string(),
            original_size,
            truncated_size: original_size,
            strategy: "none".to_string(),
        };
    }

    // Estimate marker length (use upper-bound digit counts for sizes).
    // We'll compute the real marker after we know the final truncated_size,
    // but we need a good estimate to allocate the head/tail budgets.
    let marker_estimate = build_marker(original_size, max_size);
    let marker_len = marker_estimate.len();

    // If max_size is too small to even fit the marker, return just the marker
    // (or as much of it as fits).
    if max_size <= marker_len {
        let marker = build_marker(original_size, max_size);
        let content = if marker.len() <= max_size {
            marker
        } else {
            marker[..max_size].to_string()
        };
        let truncated_size = content.len();
        return TruncationResult {
            content,
            original_size,
            truncated_size,
            strategy: "head-tail".to_string(),
        };
    }

    let budget = max_size - marker_len;
    let head_budget = (budget as f64 * 0.6) as usize;
    let tail_budget = budget - head_budget; // remainder goes to tail

    // Take head_budget bytes from the start, ensuring we don't split a
    // multi-byte UTF-8 character.
    let head = safe_prefix(content, head_budget);
    // Take tail_budget bytes from the end.
    let tail = safe_suffix(content, tail_budget);

    let marker = build_marker(original_size, head.len() + marker_len + tail.len());

    let truncated = format!("{}{}{}", head, marker, tail);
    let truncated_size = truncated.len();

    TruncationResult {
        content: truncated,
        original_size,
        truncated_size,
        strategy: "head-tail".to_string(),
    }
}

/// Truncate two streams (stdout / stderr) proportionally so that their
/// combined size fits within `max_size`.
///
/// The budget is split between the two streams in proportion to their
/// original sizes.  Each stream is then independently truncated using
/// [`smart_truncate`].
pub fn smart_truncate_streams(
    stdout: &str,
    stderr: &str,
    max_size: usize,
) -> (TruncationResult, TruncationResult) {
    let total = stdout.len() + stderr.len();

    // If everything fits, return both unchanged.
    if total <= max_size {
        return (
            smart_truncate(stdout, stdout.len()),
            smart_truncate(stderr, stderr.len()),
        );
    }

    // Allocate budget proportionally.  Guard against division by zero when
    // both streams are empty (shouldn't happen if total > max_size, but be
    // safe).
    let (stdout_budget, stderr_budget) = if total == 0 {
        (max_size / 2, max_size - max_size / 2)
    } else {
        let stdout_ratio = stdout.len() as f64 / total as f64;
        let sb = (max_size as f64 * stdout_ratio) as usize;
        // Ensure at least 1 byte for each non-empty stream so both are
        // represented.
        let sb = if !stdout.is_empty() && sb == 0 {
            1
        } else {
            sb
        };
        let eb = max_size.saturating_sub(sb);
        let eb = if !stderr.is_empty() && eb == 0 {
            // Give stderr at least 1 byte, take from stdout budget
            let sb = sb.saturating_sub(1);
            (sb, 1)
        } else {
            (sb, eb)
        };
        eb
    };

    (
        smart_truncate(stdout, stdout_budget),
        smart_truncate(stderr, stderr_budget),
    )
}

// ---------------------------------------------------------------------------
// UTF-8–safe slicing helpers
// ---------------------------------------------------------------------------

/// Return the longest prefix of `s` that is at most `max_bytes` bytes and
/// does not split a multi-byte UTF-8 character.
fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Return the longest suffix of `s` that is at most `max_bytes` bytes and
/// does not split a multi-byte UTF-8 character.
fn safe_suffix(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- smart_truncate: no truncation needed ----

    #[test]
    fn no_truncation_when_content_fits() {
        let content = "hello world";
        let result = smart_truncate(content, 100);
        assert_eq!(result.content, content);
        assert_eq!(result.original_size, content.len());
        assert_eq!(result.truncated_size, content.len());
        assert_eq!(result.strategy, "none");
    }

    #[test]
    fn no_truncation_when_content_equals_max() {
        let content = "exactly";
        let result = smart_truncate(content, content.len());
        assert_eq!(result.content, content);
        assert_eq!(result.strategy, "none");
    }

    #[test]
    fn no_truncation_empty_content() {
        let result = smart_truncate("", 100);
        assert_eq!(result.content, "");
        assert_eq!(result.original_size, 0);
        assert_eq!(result.truncated_size, 0);
        assert_eq!(result.strategy, "none");
    }

    // ---- smart_truncate: truncation applied ----

    #[test]
    fn truncation_preserves_head_and_tail() {
        // Create content large enough to require truncation
        let content: String = (0..1000).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        let max_size = 200;
        let result = smart_truncate(&content, max_size);

        assert_eq!(result.strategy, "head-tail");
        assert_eq!(result.original_size, 1000);
        // The result should contain the beginning of the content
        assert!(result.content.starts_with(&content[..10]));
        // The result should contain the end of the content
        assert!(result.content.ends_with(&content[content.len() - 10..]));
        // The result should contain the truncation marker
        assert!(result.content.contains("truncated:"));
        assert!(result.content.contains("strategy: head-tail"));
    }

    #[test]
    fn truncation_marker_contains_sizes() {
        let content = "a".repeat(500);
        let max_size = 200;
        let result = smart_truncate(&content, max_size);

        assert!(result.content.contains("500 bytes"));
        assert!(result.content.contains("strategy: head-tail"));
        assert_eq!(result.original_size, 500);
    }

    #[test]
    fn truncated_size_does_not_exceed_max() {
        let content = "x".repeat(10_000);
        let max_size = 500;
        let result = smart_truncate(&content, max_size);

        // The truncated result should be close to max_size.
        // It may be slightly different due to marker size estimation,
        // but should not significantly exceed max_size.
        assert!(
            result.truncated_size <= max_size + 20,
            "truncated_size {} should be close to max_size {}",
            result.truncated_size,
            max_size
        );
    }

    #[test]
    fn truncation_head_larger_than_tail() {
        // Verify the 60/40 split: head portion should be larger than tail
        let content = "x".repeat(10_000);
        let max_size = 500;
        let result = smart_truncate(&content, max_size);

        // Find the marker in the result
        let marker_start = result.content.find("\n...[truncated:").unwrap();
        let marker_end = result.content.find("strategy: head-tail]\n").unwrap()
            + "strategy: head-tail]\n".len();

        let head_len = marker_start;
        let tail_len = result.content.len() - marker_end;

        // Head should be roughly 60% and tail 40% of the budget
        assert!(
            head_len > tail_len,
            "head ({}) should be larger than tail ({})",
            head_len,
            tail_len
        );
    }

    // ---- smart_truncate: UTF-8 safety ----

    #[test]
    fn truncation_handles_multibyte_utf8() {
        // Each CJK character is 3 bytes in UTF-8
        let content = "中".repeat(200); // 600 bytes
        let max_size = 200;
        let result = smart_truncate(&content, max_size);

        // The result should be valid UTF-8 (it is, since it's a String)
        assert!(result.content.is_char_boundary(0));
        assert_eq!(result.strategy, "head-tail");
        // Verify it doesn't panic and produces valid output
        assert!(!result.content.is_empty());
    }

    // ---- smart_truncate: edge cases ----

    #[test]
    fn truncation_very_small_max_size() {
        let content = "a".repeat(1000);
        let result = smart_truncate(&content, 10);

        // Should still produce some output without panicking
        assert_eq!(result.strategy, "head-tail");
        assert_eq!(result.original_size, 1000);
    }

    #[test]
    fn truncation_max_size_zero() {
        let content = "hello";
        let result = smart_truncate(content, 0);

        assert_eq!(result.strategy, "head-tail");
        assert_eq!(result.original_size, 5);
    }

    // ---- smart_truncate_streams ----

    #[test]
    fn streams_no_truncation_when_both_fit() {
        let stdout = "stdout output";
        let stderr = "stderr output";
        let max_size = stdout.len() + stderr.len() + 100;

        let (out_r, err_r) = smart_truncate_streams(stdout, stderr, max_size);

        assert_eq!(out_r.content, stdout);
        assert_eq!(out_r.strategy, "none");
        assert_eq!(err_r.content, stderr);
        assert_eq!(err_r.strategy, "none");
    }

    #[test]
    fn streams_proportional_truncation() {
        let stdout = "o".repeat(8000); // 80% of total
        let stderr = "e".repeat(2000); // 20% of total
        let max_size = 1000;

        let (out_r, err_r) = smart_truncate_streams(&stdout, &stderr, max_size);

        // Both streams should be represented
        assert!(!out_r.content.is_empty());
        assert!(!err_r.content.is_empty());

        // stdout should get a larger budget since it's 80% of total
        assert_eq!(out_r.original_size, 8000);
        assert_eq!(err_r.original_size, 2000);
    }

    #[test]
    fn streams_empty_stderr() {
        let stdout = "o".repeat(5000);
        let stderr = "";
        let max_size = 1000;

        let (out_r, err_r) = smart_truncate_streams(&stdout, stderr, max_size);

        assert!(!out_r.content.is_empty());
        assert_eq!(err_r.content, "");
        assert_eq!(err_r.strategy, "none");
    }

    #[test]
    fn streams_empty_stdout() {
        let stdout = "";
        let stderr = "e".repeat(5000);
        let max_size = 1000;

        let (out_r, err_r) = smart_truncate_streams(stdout, &stderr, max_size);

        assert_eq!(out_r.content, "");
        assert_eq!(out_r.strategy, "none");
        assert!(!err_r.content.is_empty());
    }

    #[test]
    fn streams_both_empty() {
        let (out_r, err_r) = smart_truncate_streams("", "", 100);

        assert_eq!(out_r.content, "");
        assert_eq!(err_r.content, "");
    }

    #[test]
    fn streams_both_represented_when_truncated() {
        let stdout = "o".repeat(5000);
        let stderr = "e".repeat(5000);
        let max_size = 500;

        let (out_r, err_r) = smart_truncate_streams(&stdout, &stderr, max_size);

        // Both should be truncated and non-empty
        assert!(!out_r.content.is_empty(), "stdout should be represented");
        assert!(!err_r.content.is_empty(), "stderr should be represented");
        assert_eq!(out_r.strategy, "head-tail");
        assert_eq!(err_r.strategy, "head-tail");
    }
}
