//! 审批响应关键词识别模块
//!
//! 用于识别用户在非 Web 端（Telegram、Discord、飞书）发送的审批响应关键词

/// 识别用户消息是否为审批响应
///
/// # 参数
/// * `text` - 用户发送的消息文本
///
/// # 返回值
/// * `Some(true)` - 批准关键词
/// * `Some(false)` - 拒绝关键词
/// * `None` - 不是审批响应
///
/// # 支持的关键词
/// ## 批准关键词（中文）
/// - 同意
/// - 批准
/// - 允许
/// - ok
/// - 好的
///
/// ## 批准关键词（英文）
/// - yes
/// - y
/// - approve
/// - accept
/// - allow
///
/// ## 拒绝关键词（中文）
/// - 拒绝
/// - 不同意
/// - 不允许
/// - 不行
///
/// ## 拒绝关键词（英文）
/// - no
/// - n
/// - reject
/// - deny
/// - decline
///
/// # 示例
/// ```
/// use synbot::channels::approval_parser::is_approval_response;
///
/// assert_eq!(is_approval_response("同意"), Some(true));
/// assert_eq!(is_approval_response("YES"), Some(true));
/// assert_eq!(is_approval_response("拒绝"), Some(false));
/// assert_eq!(is_approval_response("no"), Some(false));
/// assert_eq!(is_approval_response("hello"), None);
/// ```
pub fn is_approval_response(text: &str) -> Option<bool> {
    let trimmed = text.trim().to_lowercase();

    // 批准关键词（中文）
    if matches!(
        trimmed.as_str(),
        "同意" | "批准" | "允许" | "ok" | "好的" | "好"
    ) {
        return Some(true);
    }

    // 批准关键词（英文）
    if matches!(
        trimmed.as_str(),
        "yes" | "y" | "approve" | "accept" | "allow"
    ) {
        return Some(true);
    }

    // 拒绝关键词（中文）
    if matches!(
        trimmed.as_str(),
        "拒绝" | "不同意" | "不允许" | "不行" | "不"
    ) {
        return Some(false);
    }

    // 拒绝关键词（英文）
    if matches!(
        trimmed.as_str(),
        "no" | "n" | "reject" | "deny" | "decline"
    ) {
        return Some(false);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chinese_approval_keywords() {
        assert_eq!(is_approval_response("同意"), Some(true));
        assert_eq!(is_approval_response("批准"), Some(true));
        assert_eq!(is_approval_response("允许"), Some(true));
        assert_eq!(is_approval_response("ok"), Some(true));
        assert_eq!(is_approval_response("好的"), Some(true));
        assert_eq!(is_approval_response("好"), Some(true));
    }

    #[test]
    fn test_english_approval_keywords() {
        assert_eq!(is_approval_response("yes"), Some(true));
        assert_eq!(is_approval_response("y"), Some(true));
        assert_eq!(is_approval_response("approve"), Some(true));
        assert_eq!(is_approval_response("accept"), Some(true));
        assert_eq!(is_approval_response("allow"), Some(true));
    }

    #[test]
    fn test_chinese_rejection_keywords() {
        assert_eq!(is_approval_response("拒绝"), Some(false));
        assert_eq!(is_approval_response("不同意"), Some(false));
        assert_eq!(is_approval_response("不允许"), Some(false));
        assert_eq!(is_approval_response("不行"), Some(false));
        assert_eq!(is_approval_response("不"), Some(false));
    }

    #[test]
    fn test_english_rejection_keywords() {
        assert_eq!(is_approval_response("no"), Some(false));
        assert_eq!(is_approval_response("n"), Some(false));
        assert_eq!(is_approval_response("reject"), Some(false));
        assert_eq!(is_approval_response("deny"), Some(false));
        assert_eq!(is_approval_response("decline"), Some(false));
    }

    #[test]
    fn test_case_insensitive() {
        // 大写
        assert_eq!(is_approval_response("YES"), Some(true));
        assert_eq!(is_approval_response("NO"), Some(false));
        assert_eq!(is_approval_response("APPROVE"), Some(true));
        assert_eq!(is_approval_response("REJECT"), Some(false));

        // 混合大小写
        assert_eq!(is_approval_response("Yes"), Some(true));
        assert_eq!(is_approval_response("No"), Some(false));
        assert_eq!(is_approval_response("Approve"), Some(true));
        assert_eq!(is_approval_response("Reject"), Some(false));
    }

    #[test]
    fn test_with_whitespace() {
        assert_eq!(is_approval_response("  yes  "), Some(true));
        assert_eq!(is_approval_response("\tno\t"), Some(false));
        assert_eq!(is_approval_response(" 同意 "), Some(true));
        assert_eq!(is_approval_response(" 拒绝 "), Some(false));
    }

    #[test]
    fn test_non_approval_messages() {
        assert_eq!(is_approval_response("hello"), None);
        assert_eq!(is_approval_response("maybe"), None);
        assert_eq!(is_approval_response("later"), None);
        assert_eq!(is_approval_response("你好"), None);
        assert_eq!(is_approval_response(""), None);
        assert_eq!(is_approval_response("   "), None);
    }

    #[test]
    fn test_partial_matches_not_recognized() {
        // 确保只匹配完整的关键词，不匹配部分
        assert_eq!(is_approval_response("yes please"), None);
        assert_eq!(is_approval_response("no way"), None);
        assert_eq!(is_approval_response("我同意你的观点"), None);
    }
}
