//! URL normalization for HTTP clients (e.g. punycode for IDN) to avoid
//! "invalid international domain name" errors from reqwest/url.

use anyhow::{Context, Result};
use url::Url;

/// Normalizes a URL string so it is safe to use with reqwest: ensures the host
/// is ASCII (converts IDN to punycode if needed). Returns the normalized URL string.
pub fn normalize_http_url(s: &str) -> Result<String> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("URL is empty");
    }

    // Try parsing first; if it succeeds and host is already ASCII, return as-is (with normalizations)
    if let Ok(u) = Url::parse(s) {
        if let Some(h) = u.host_str() {
            if h.chars().all(|c| c.is_ascii()) {
                return Ok(u.to_string());
            }
            // Host has non-ASCII: convert to punycode and rebuild
            let ascii_host = idna::domain_to_ascii(h).map_err(|e| {
                anyhow::anyhow!("invalid international domain name: {}", e)
            })?;
            let mut new = u.clone();
            if new.set_host(Some(&ascii_host)).is_ok() {
                return Ok(new.to_string());
            }
        }
        return Ok(u.to_string());
    }

    // Parse failed (e.g. IDN rejected); extract scheme and authority manually and convert host
    let (scheme, rest) = s
        .split_once("://")
        .context("URL must include scheme (e.g. https://)")?;
    let authority_end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let authority = rest[..authority_end].trim_end();
    let path_etc = rest[authority_end..].trim_start_matches('/');

    let (host, port_part) = match authority.find(']') {
        Some(_) => {
            // IPv6 [::1] or [::1]:port
            let bracket_end = authority.find(']').unwrap_or(0) + 1;
            let host = authority[..bracket_end].trim_start_matches('[').trim_end_matches(']');
            let port_part = authority[bracket_end..].trim_start_matches(':');
            (host, port_part)
        }
        None => {
            let colon = authority.find(':');
            let (host, port_part) = match colon {
                Some(i) => (authority[..i].trim(), authority[i + 1..].trim()),
                None => (authority, ""),
            };
            (host, port_part)
        }
    };

    if host.is_empty() {
        anyhow::bail!("URL has no host");
    }

    // Only convert to punycode if host has non-ASCII (IDN); leave IPv4/IPv6 as-is
    let ascii_host = if host.chars().all(|c| c.is_ascii()) {
        host.to_string()
    } else {
        idna::domain_to_ascii(host).map_err(|e| {
            anyhow::anyhow!("invalid international domain name: {}", e)
        })?
    };

    let authority_new = if port_part.is_empty() {
        ascii_host
    } else {
        format!("{}:{}", ascii_host, port_part)
    };

    let full = if path_etc.is_empty() {
        format!("{}://{}", scheme, authority_new)
    } else {
        format!("{}://{}/{}", scheme, authority_new, path_etc)
    };

    // Validate
    Url::parse(&full).context("normalized URL failed to parse")?;
    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_ascii_url_unchanged() {
        let u = "https://api.openai.com/v1";
        assert_eq!(normalize_http_url(u).unwrap(), u);
    }

    #[test]
    fn normalizes_idn_to_punycode() {
        let u = "https://例子.中国/path";
        let out = normalize_http_url(u).unwrap();
        assert!(out.starts_with("https://xn--"));
        assert!(out.contains("/path"));
    }

    #[test]
    fn empty_rejected() {
        assert!(normalize_http_url("").is_err());
        assert!(normalize_http_url("   ").is_err());
    }
}
