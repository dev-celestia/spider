//! Port of the reference crawler `pkg/navigation` (Request/Response) and `pkg/output` (Result) types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

/// Lowercased HTTP headers (reference crawler `navigation.Headers` marshals keys lowercased).
pub type Headers = HashMap<String, String>;

/// Extracted form metadata (reference crawler `navigation.Form`, used by `-fx` form extraction).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Form {
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub method: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub action: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub enctype: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parameters: Vec<String>,
}

/// A navigation request for the crawler (reference crawler `navigation.Request`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Request {
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub method: String,
    /// Target endpoint URL.
    #[serde(skip_serializing_if = "String::is_empty", default, rename = "endpoint")]
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub body: String,
    #[serde(skip)]
    pub depth: i32,
    #[serde(skip)]
    pub skip_validation: bool,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub headers: HashMap<String, String>,
    /// HTML tag the URL was found in (a, link, script, iframe, header, form, ...).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub tag: String,
    /// Attribute the URL was found in (href, src, action, ...).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub attribute: String,
    #[serde(skip)]
    pub root_hostname: String,
    /// Absolute URL of the page the navigation was discovered on.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub source: String,
    #[serde(skip_serializing_if = "HashMap::is_empty", default, rename = "custom_fields")]
    pub custom_fields: HashMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub raw: String,
}

impl Request {
    /// Request identity used for dedup: URL for GET, `URL:body` for POST
    /// (reference crawler `Request.RequestURL`).
    pub fn request_url(&self) -> String {
        match self.method.as_str() {
            "GET" => self.url.clone(),
            "POST" => format!("{}:{}", self.url, self.body),
            _ => String::new(),
        }
    }

    /// Build a GET navigation request from a relative path discovered in a response
    /// (reference crawler `NewNavigationRequestURLFromResponse`).
    pub fn from_response(
        path: &str,
        source: &str,
        tag: &str,
        attribute: &str,
        resp: &Response,
    ) -> Request {
        Request {
            method: "GET".to_string(),
            url: resp.absolute_url(path),
            root_hostname: resp.root_hostname.clone(),
            depth: resp.depth,
            source: source.to_string(),
            attribute: attribute.to_string(),
            tag: tag.to_string(),
            ..Default::default()
        }
    }
}

/// A response generated from crawler navigation (reference crawler `navigation.Response`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Response {
    #[serde(skip)]
    pub depth: i32,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub status_code: i32,
    #[serde(skip_serializing_if = "Headers::is_empty", default)]
    pub headers: Headers,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub body: String,
    #[serde(skip_serializing_if = "is_zero_i64", default)]
    pub content_length: i64,    #[serde(skip)]
    pub root_hostname: String,
    /// Absolute URL of the page this response was served from (celestia keeps
    /// this on the http request; we store it directly).
    #[serde(skip)]
    pub source: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub technologies: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub raw: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub forms: Vec<Form>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub xhr_requests: Vec<Request>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub stored_response_path: String,
    #[serde(rename = "knowledgebase", skip_serializing_if = "Option::is_none", default)]
    pub knowledge_base: Option<serde_json::Value>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

impl Response {
    /// Resolve `path` against the response's source URL, dropping fragments
    /// (reference crawler `Response.AbsoluteURL`).
    pub fn absolute_url(&self, path: &str) -> String {
        if path.starts_with('#') {
            return String::new();
        }
        let base = match Url::parse(&self.source) {
            Ok(b) => b,
            Err(_) => return String::new(),
        };
        match base.join(path) {
            Ok(mut u) => {
                u.set_fragment(None);
                u.to_string()
            }
            Err(_) => String::new(),
        }
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status_code)
    }

    /// Set the originating page URL so `absolute_url` can resolve relative links.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self
    }
}

/// Result of crawling (reference crawler `output.Result`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Result {
    /// RFC3339 timestamp of when the result was produced.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub request: Option<Request>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub response: Option<Response>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub error: String,
}

impl Result {
    pub fn has_response(&self) -> bool {
        self.response.is_some()
    }

    /// Convenience accessor for the request URL.
    pub fn url(&self) -> &str {
        self.request.as_ref().map(|r| r.url.as_str()).unwrap_or("")
    }
}

/// Current timestamp in RFC3339 format for [`Result::timestamp`].
pub fn now_rfc3339() -> String {
    // No chrono dependency: build from UNIX epoch seconds via std.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days_since_epoch = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (y, mo, d) = civil_from_days(days_since_epoch);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's civil-from-days algorithm (no external date dependency).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(base: &str) -> Response {
        Response::default().with_source(base)
    }

    #[test]
    fn test_absolute_url() {
        let r = resp("https://example.com/page/index.html");
        assert_eq!(r.absolute_url("/about"), "https://example.com/about");
        assert_eq!(r.absolute_url("other.html"), "https://example.com/page/other.html");
        assert_eq!(r.absolute_url("#frag"), "");
        assert_eq!(
            r.absolute_url("https://other.com/x"),
            "https://other.com/x"
        );
    }

    #[test]
    fn test_request_url_dedup_identity() {
        let mut req = Request {
            method: "POST".into(),
            url: "https://x.com".into(),
            body: "a=1".into(),
            ..Default::default()
        };
        assert_eq!(req.request_url(), "https://x.com:a=1");
        req.method = "GET".into();
        assert_eq!(req.request_url(), "https://x.com");
    }

    #[test]
    fn test_from_response_fields() {
        let r = resp("https://example.com/a");
        let req = Request::from_response("/b", "https://example.com/a", "a", "href", &r);
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://example.com/b");
        assert_eq!(req.tag, "a");
        assert_eq!(req.attribute, "href");
    }

    #[test]
    fn test_timestamp_format() {
        let ts = now_rfc3339();
        // 2026-xx-xxTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn test_is_redirect() {
        let mut r = resp("https://x.com");
        r.status_code = 302;
        assert!(r.is_redirect());
        r.status_code = 200;
        assert!(!r.is_redirect());
    }
}
