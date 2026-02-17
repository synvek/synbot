//! Directive parsing for the `@@role content` syntax.
//!
//! The [`DirectiveParser`] extracts one or more [`Directive`]s from a user
//! message. Each directive targets either a named sub-role or the Commander
//! (when no `@@` prefix is present).

/// A single parsed directive extracted from a user message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// Target role name. `None` means the message is for the Commander.
    pub target: Option<String>,
    /// The directive content (trimmed).
    pub content: String,
}

/// Stateless directive parser.
pub struct DirectiveParser;

impl DirectiveParser {
    /// Parse a message into one or more directives.
    ///
    /// - No `@@` prefix: single directive with `target: None`.
    /// - `@@role content`: directive with `target: Some("role")`.
    /// - Multiple `@@role` segments each produce their own directive.
    /// - Text before the first `@@` goes to Commander (`target: None`).
    /// - Role names must match `[a-zA-Z0-9_]+`.
    ///
    /// In group chats the channel strips the bot mention (e.g. `@机器人`) and 0+ spaces
    /// before passing content here; the remainder may start with `@@dev` or another
    /// `@@role`, and is parsed as above (no special handling needed).
    pub fn parse(input: &str) -> Vec<Directive> {
        // Find all positions where "@@" occurs
        let marker = "@@";
        let positions: Vec<usize> = input
            .match_indices(marker)
            .map(|(idx, _)| idx)
            .collect();

        // No @@ at all -> entire message goes to Commander
        if positions.is_empty() {
            return vec![Directive {
                target: None,
                content: input.trim().to_string(),
            }];
        }

        let mut directives = Vec::new();

        // Text before the first @@ goes to Commander
        if positions[0] > 0 {
            let prefix = input[..positions[0]].trim();
            if !prefix.is_empty() {
                directives.push(Directive {
                    target: None,
                    content: prefix.to_string(),
                });
            }
        }

        // Process each @@ segment
        for (i, &start) in positions.iter().enumerate() {
            let after_marker = start + marker.len();
            let end = if i + 1 < positions.len() {
                positions[i + 1]
            } else {
                input.len()
            };

            let segment = &input[after_marker..end];

            // Try to extract a valid role name at the start of the segment
            let role_end = segment
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(segment.len());

            let role_candidate = &segment[..role_end];

            if role_candidate.is_empty() {
                // @@ not followed by a valid role name -> treat as Commander content
                let text = format!("{}{}", marker, segment).trim().to_string();
                if !text.is_empty() {
                    directives.push(Directive {
                        target: None,
                        content: text,
                    });
                }
            } else {
                let content = segment[role_end..].trim().to_string();
                directives.push(Directive {
                    target: Some(role_candidate.to_string()),
                    content,
                });
            }
        }

        // If we found @@ markers but produced no directives (e.g. "@@" alone),
        // return a single empty Commander directive.
        if directives.is_empty() {
            directives.push(Directive {
                target: None,
                content: String::new(),
            });
        }

        directives
    }

    /// Format a list of directives back into text form.
    ///
    /// This is the inverse of [`parse`](Self::parse) and is used for
    /// round-trip consistency verification.
    pub fn format(directives: &[Directive]) -> String {
        let mut parts = Vec::new();

        for d in directives {
            match &d.target {
                None => {
                    parts.push(d.content.clone());
                }
                Some(role) => {
                    if d.content.is_empty() {
                        parts.push(format!("@@{}", role));
                    } else {
                        parts.push(format!("@@{} {}", role, d.content));
                    }
                }
            }
        }

        parts.join(" ")
    }
}


// ── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- Empty / no-directive messages ---

    #[test]
    fn empty_input_returns_commander_directive() {
        let result = DirectiveParser::parse("");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, None);
        assert_eq!(result[0].content, "");
    }

    #[test]
    fn whitespace_only_returns_commander_directive() {
        let result = DirectiveParser::parse("   ");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, None);
        assert_eq!(result[0].content, "");
    }

    #[test]
    fn plain_text_returns_commander_directive() {
        let result = DirectiveParser::parse("hello world");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, None);
        assert_eq!(result[0].content, "hello world");
    }

    // --- Single @@ directive ---

    #[test]
    fn single_directive_with_content() {
        let result = DirectiveParser::parse("@@ui_designer create a button");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, Some("ui_designer".to_string()));
        assert_eq!(result[0].content, "create a button");
    }

    #[test]
    fn single_directive_no_content() {
        let result = DirectiveParser::parse("@@ui_designer");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, Some("ui_designer".to_string()));
        assert_eq!(result[0].content, "");
    }

    #[test]
    fn single_directive_with_extra_spaces() {
        let result = DirectiveParser::parse("@@role1   lots of   spaces  ");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, Some("role1".to_string()));
        assert_eq!(result[0].content, "lots of   spaces");
    }

    // --- Multiple @@ directives ---

    #[test]
    fn multiple_directives() {
        let result =
            DirectiveParser::parse("@@ui_designer make a form @@product_manager review spec");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].target, Some("ui_designer".to_string()));
        assert_eq!(result[0].content, "make a form");
        assert_eq!(result[1].target, Some("product_manager".to_string()));
        assert_eq!(result[1].content, "review spec");
    }

    // --- Text before first @@ ---

    #[test]
    fn text_before_directive_goes_to_commander() {
        let result = DirectiveParser::parse("hello @@role1 do something");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].target, None);
        assert_eq!(result[0].content, "hello");
        assert_eq!(result[1].target, Some("role1".to_string()));
        assert_eq!(result[1].content, "do something");
    }

    // --- Role name validation ---

    #[test]
    fn role_name_alphanumeric_and_underscore() {
        let result = DirectiveParser::parse("@@Role_123 content");
        assert_eq!(result[0].target, Some("Role_123".to_string()));
        assert_eq!(result[0].content, "content");
    }

    #[test]
    fn bare_double_at_treated_as_commander() {
        // "@@" followed by a space (no valid role name)
        let result = DirectiveParser::parse("@@ some text");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, None);
        // The @@ and text are preserved
        assert!(result[0].content.contains("@@"));
    }

    /// Group: after stripping "@机器人" and 0+ spaces, content may be "@@dev ..." or "  @@dev ...".
    #[test]
    fn leading_spaces_before_at_at_role_parsed_as_role() {
        let result = DirectiveParser::parse("  @@dev list files");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, Some("dev".to_string()));
        assert_eq!(result[0].content, "list files");
    }

    #[test]
    fn at_at_dev_immediately_after_bot_mention_still_parsed() {
        // Simulates content after stripping "@机器人" (no spaces between @bot and @@dev)
        let result = DirectiveParser::parse("@@dev run tests");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].target, Some("dev".to_string()));
        assert_eq!(result[0].content, "run tests");
    }

    // --- Format ---

    #[test]
    fn format_commander_only() {
        let directives = vec![Directive {
            target: None,
            content: "hello".to_string(),
        }];
        assert_eq!(DirectiveParser::format(&directives), "hello");
    }

    #[test]
    fn format_single_role() {
        let directives = vec![Directive {
            target: Some("role1".to_string()),
            content: "do stuff".to_string(),
        }];
        assert_eq!(DirectiveParser::format(&directives), "@@role1 do stuff");
    }

    #[test]
    fn format_multiple_roles() {
        let directives = vec![
            Directive {
                target: Some("role1".to_string()),
                content: "task1".to_string(),
            },
            Directive {
                target: Some("role2".to_string()),
                content: "task2".to_string(),
            },
        ];
        assert_eq!(
            DirectiveParser::format(&directives),
            "@@role1 task1 @@role2 task2"
        );
    }

    #[test]
    fn format_commander_then_role() {
        let directives = vec![
            Directive {
                target: None,
                content: "hello".to_string(),
            },
            Directive {
                target: Some("role1".to_string()),
                content: "do it".to_string(),
            },
        ];
        assert_eq!(
            DirectiveParser::format(&directives),
            "hello @@role1 do it"
        );
    }

    #[test]
    fn format_role_no_content() {
        let directives = vec![Directive {
            target: Some("role1".to_string()),
            content: String::new(),
        }];
        assert_eq!(DirectiveParser::format(&directives), "@@role1");
    }

    // --- Roundtrip ---

    #[test]
    fn roundtrip_plain_text() {
        let input = "hello world";
        let parsed = DirectiveParser::parse(input);
        let formatted = DirectiveParser::format(&parsed);
        let reparsed = DirectiveParser::parse(&formatted);
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn roundtrip_single_directive() {
        let input = "@@role1 do something";
        let parsed = DirectiveParser::parse(input);
        let formatted = DirectiveParser::format(&parsed);
        let reparsed = DirectiveParser::parse(&formatted);
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn roundtrip_multiple_directives() {
        let input = "@@role1 task one @@role2 task two";
        let parsed = DirectiveParser::parse(input);
        let formatted = DirectiveParser::format(&parsed);
        let reparsed = DirectiveParser::parse(&formatted);
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn roundtrip_commander_then_role() {
        let input = "hello @@role1 do it";
        let parsed = DirectiveParser::parse(input);
        let formatted = DirectiveParser::format(&parsed);
        let reparsed = DirectiveParser::parse(&formatted);
        assert_eq!(parsed, reparsed);
    }
}
