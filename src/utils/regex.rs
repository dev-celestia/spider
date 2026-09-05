//! Port of the reference crawler `pkg/utils/regex.go` and tag-parsing helpers from
//! `pkg/utils/utils.go` — endpoint extraction regexes and Link/Refresh/srcset
//! tag parsing.

use regex::Regex;
use std::sync::OnceLock;

/// reference crawler `pageBodyRegex` — extracts endpoints from HTML page bodies.
fn page_body_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r"(?:(",
            r"(?:\.{1,2}/[A-Za-z0-9\-_/\\?&@\.?=%]+)",
            r"|(https?://[A-Za-z0-9_\-\.]+([\.]{0,2})?/[A-Za-z0-9\-_/\\?&@\.?=%]+)",
            r"|(/[A-Za-z0-9\-_/\\?&@\.%]+\.(?:aspx?|action|cfm|cgi|do|pl|css|x?html?|js(?:p|on)?|pdf|php5?|py|rss))",
            r"|([A-Za-z0-9\-_?&@\.%]+/[A-Za-z0-9/\\\-_?&@\.%]+\.(?:aspx?|action|cfm|cgi|do|pl|css|x?html?|js(?:p|on)?|pdf|php5?|py|rss))",
            r"))"
        ))
        .expect("page body regex")
    })
}

/// reference crawler `relativeEndpointsRegex` — finds endpoints inside JS content.
fn relative_endpoints_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // celestia JsC0..JsC3 alternatives wrapped in a quote/whitespace boundary.
        let jsc0 = r#"(?:https?://[A-Za-z0-9_\-.]+(?::\d{1,5})?(?:\.{1,2})?/[A-Za-z0-9/\-_.%]+(?:(?:\?|#)[^"'\s]*)?)"#;
        let jsc1 = r#"(?:\.{1,2}/)?[a-zA-Z0-9\-_/\\%]+\.(?:aspx?|js(?:on|p)?|html|php5?|action|do)(?:(?:\?|#)[^"'\s]*)?"#;
        let jsc2 = r#"(?:\.{0,2}/)[a-zA-Z0-9\-_/\\%]+(?:/|\\)[a-zA-Z0-9\-_]{3,}(?:(?:\?|#)[^"'\s]*)?"#;
        let jsc3 = r"(?:\.{0,2})[a-zA-Z0-9\-_/\\%]{3,}/";
        Regex::new(&format!(
            r#"(?:"|'|\s)\s*({}|{}|{}|{})\s*(?:"|'|\s)"#,
            jsc0, jsc1, jsc2, jsc3
        ))
        .expect("relative endpoints regex")
    })
}

/// Extract body endpoints from a data item (reference crawler `ExtractBodyEndpoints`).
pub fn extract_body_endpoints(data: &str) -> Vec<String> {
    let mut matches = Vec::new();
    let mut unique = std::collections::HashSet::new();
    for caps in page_body_regex().captures_iter(data) {
        if let Some(m) = caps.get(1) {
            if unique.insert(m.as_str().to_string()) {
                matches.push(m.as_str().to_string());
            }
        }
    }
    matches
}

/// Extract relative endpoints from JS content (reference crawler `ExtractRelativeEndpoints`).
pub fn extract_relative_endpoints(data: &str) -> Vec<String> {
    let mut matches = Vec::new();
    let mut unique = std::collections::HashSet::new();
    for caps in relative_endpoints_regex().captures_iter(data) {
        if let Some(m) = caps.get(1) {
            if unique.insert(m.as_str().to_string()) {
                matches.push(m.as_str().to_string());
            }
        }
    }
    matches
}

/// Parse an `img srcset` attribute returning candidate URLs (reference crawler `ParseSRCSetTag`).
pub fn parse_srcset_tag(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            // Each descriptor entry is "url [descriptor]"
            part.split_whitespace().next().map(|s| s.to_string())
        })
        .collect()
}

/// Parse an HTTP `Link` header returning found URLs
/// (reference crawler `ParseLinkTag`, inspired by tomnomnom/linkheader).
pub fn parse_link_tag(value: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for chunk in value.split(',') {
        for piece in chunk.split(';') {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            if piece.starts_with('<') && piece.ends_with('>') {
                urls.push(piece.trim_matches(|c| c == '<' || c == '>').to_string());
            }
        }
    }
    urls
}

/// Parse an HTTP `Refresh` header returning the target URL
/// (reference crawler `ParseRefreshTag`).
pub fn parse_refresh_tag(value: &str) -> String {
    let chunks: Vec<&str> = value.split("url=").collect();
    if chunks.len() < 2 {
        return String::new();
    }
    let chunk = chunks[1].trim_end_matches(';');
    if chunk.is_empty() {
        return String::new();
    }
    chunk.trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// reference crawler `WebUserAgent` — the Chrome web user agent used by default.
pub fn web_user_agent() -> &'static str {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_body_endpoints() {
        let data = r#"<a href="/admin/login.php">x</a> <a href="https://other.com/path/page.html">y</a>"#;
        let eps = extract_body_endpoints(data);
        assert!(eps.contains(&"/admin/login.php".to_string()), "{eps:?}");
        assert!(eps.contains(&"https://other.com/path/page.html".to_string()));
    }

    #[test]
    fn test_extract_relative_endpoints_js() {
        let js = r#"var u="https://api.example.com/v1/users?id=5"; var p='/api/config.json'; fetch("./data.json")"#;
        let eps = extract_relative_endpoints(js);
        assert!(
            eps.iter().any(|e| e.contains("api.example.com/v1/users")),
            "{eps:?}"
        );
        assert!(eps.iter().any(|e| e.contains("config.json")), "{eps:?}");
        assert!(eps.iter().any(|e| e.contains("data.json")), "{eps:?}");
    }

    #[test]
    fn test_parse_srcset() {
        let urls = parse_srcset_tag("a.png 1x, b.png 2x, c.png 3x");
        assert_eq!(urls, vec!["a.png", "b.png", "c.png"]);
    }

    #[test]
    fn test_parse_link_tag() {
        let urls = parse_link_tag("<https://x.com/1>; rel=next, <https://x.com/2>; rel=prev");
        assert_eq!(urls, vec!["https://x.com/1", "https://x.com/2"]);
    }

    #[test]
    fn test_parse_refresh_tag() {
        assert_eq!(parse_refresh_tag("5; url=/target.html"), "/target.html");
        assert_eq!(parse_refresh_tag("5"), "");
    }
}
