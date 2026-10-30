//! A deliberately small XML tokenizer — enough to read OOXML parts.
//!
//! Office documents are machine-generated, so a full XML parser is not needed to pull text out of
//! them: element names, attributes and text nodes are enough, with comments, CDATA, processing
//! instructions and doctypes skipped. Anything malformed simply stops the token stream instead of
//! failing the whole preview — a truncated document still shows its first paragraphs.

/// One piece of an XML document, with namespace prefixes stripped from element names.
#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    /// Opening tag: local name (`w:t` → `t`), the raw attribute string, and whether it self-closes.
    Start {
        name: &'a str,
        attrs: &'a str,
        self_closing: bool,
    },
    /// Closing tag.
    End { name: &'a str },
    /// Text between tags (still XML-escaped).
    Text(&'a str),
}

/// Tokenize an XML document. The iterator ends quietly on malformed markup.
pub fn tokenize(xml: &str) -> Tokenizer<'_> {
    Tokenizer { rest: xml }
}

pub struct Tokenizer<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        loop {
            let rest = self.rest;
            if rest.is_empty() {
                return None;
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let (text, tail) = rest.split_at(end);
                self.rest = tail;
                if text.is_empty() {
                    continue;
                }
                return Some(Token::Text(text));
            }
            // Markup that carries no text: skip it and keep looking.
            if rest.starts_with("<?") {
                let end = rest.find("?>")? + 2;
                self.rest = &rest[end..];
                continue;
            }
            if rest.starts_with("<!--") {
                let end = rest.find("-->")? + 3;
                self.rest = &rest[end..];
                continue;
            }
            if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
                let end = cdata.find("]]>")?;
                self.rest = &cdata[end + 3..];
                let text = &cdata[..end];
                if text.is_empty() {
                    continue;
                }
                return Some(Token::Text(text));
            }
            if rest.starts_with("<!") {
                let end = rest.find('>')? + 1;
                self.rest = &rest[end..];
                continue;
            }

            let end = rest.find('>')?;
            self.rest = &rest[end + 1..];
            let inner = rest[1..end].trim();
            if let Some(name) = inner.strip_prefix('/') {
                return Some(Token::End {
                    name: local_name(name.trim()),
                });
            }
            let self_closing = inner.ends_with('/');
            let inner = inner.trim_end_matches('/').trim_end();
            let (name, attrs) = match inner.find(char::is_whitespace) {
                Some(idx) => (&inner[..idx], inner[idx..].trim()),
                None => (inner, ""),
            };
            if name.is_empty() {
                continue;
            }
            return Some(Token::Start {
                name: local_name(name),
                attrs,
                self_closing,
            });
        }
    }
}

/// `w:t` → `t`, `t` → `t`. Producers are free to pick their own prefixes.
fn local_name(name: &str) -> &str {
    match name.rsplit_once(':') {
        Some((_, local)) => local,
        None => name,
    }
}

/// Value of `key` in a raw attribute string. Namespaced keys (`r:id`) match literally.
pub fn attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while let Some(pos) = rest.find(key) {
        let at_boundary = rest[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace());
        let after = &rest[pos + key.len()..];
        if at_boundary {
            if let Some(quoted) = after.trim_start().strip_prefix('=') {
                let quoted = quoted.trim_start();
                let quote = quoted.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let body = &quoted[1..];
                    return body.find(quote).map(|end| &body[..end]);
                }
            }
        }
        rest = after;
    }
    None
}

/// Decode the five predefined entities plus numeric character references.
pub fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(end) = tail.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity.strip_prefix('#').and_then(|number| {
                let code = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => number.parse::<u32>().ok()?,
                };
                char::from_u32(code)
            }),
        };
        match decoded {
            Some(ch) => out.push(ch),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Collapse runs of whitespace so `word wrapping<br/>in the source` reads as one line.
pub fn squash_whitespace(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut space = false;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(xml: &str) -> Vec<&str> {
        tokenize(xml)
            .filter_map(|token| match token {
                Token::Start { name, .. } => Some(name),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn strips_prefixes_and_skips_comments_and_declarations() {
        let xml =
            r#"<?xml version="1.0"?><!-- hi --><w:document><w:p><w:t>a</w:t></w:p></w:document>"#;
        assert_eq!(
            starts(xml),
            vec!["document", "p", "t"],
            "declaration and comment must not become elements"
        );
    }

    #[test]
    fn reads_text_entities_and_cdata() {
        let xml = "<p>a &amp; b &#65;<![CDATA[<raw>]]>c</p>";
        let text: String = tokenize(xml)
            .filter_map(|token| match token {
                Token::Text(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(decode_entities(&text), "a & b A<raw>c");
    }

    #[test]
    fn reports_self_closing_tags() {
        let tokens: Vec<Token> = tokenize("<a/><b />").collect();
        assert!(matches!(
            tokens[0],
            Token::Start {
                name: "a",
                self_closing: true,
                ..
            }
        ));
        assert!(matches!(
            tokens[1],
            Token::Start {
                name: "b",
                self_closing: true,
                ..
            }
        ));
    }

    #[test]
    fn stops_on_malformed_markup_instead_of_panicking() {
        let tokens: Vec<Token> = tokenize("<a><b>text").collect();
        assert_eq!(tokens.len(), 3, "the unterminated tag is simply dropped");
        assert!(matches!(tokens[2], Token::Text("text")));
    }

    #[test]
    fn attr_matches_whole_names_only() {
        let attrs = r#"r="B3" t="s" xt:id="no""#;
        assert_eq!(attr(attrs, "r"), Some("B3"));
        assert_eq!(attr(attrs, "t"), Some("s"));
        assert_eq!(attr(attrs, "id"), None, "must not match inside `xt:id`");
        assert_eq!(attr(attrs, "r:id"), None);
        assert_eq!(attr(r#"r:id="rId1""#, "r:id"), Some("rId1"));
    }

    #[test]
    fn squash_whitespace_collapses_runs() {
        assert_eq!(squash_whitespace("  a \n\t b  "), "a b");
        assert_eq!(squash_whitespace("   "), "");
    }
}
