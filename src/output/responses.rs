//! Port of the reference crawler `pkg/output/responses.go` — storing raw HTTP
//! requests/responses to per-host files with an index.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use md5::{Digest, Md5};

use crate::types::result::Result;

/// Hash of a URL used as the stored response file name
/// (the reference crawler uses sha1; md5 keeps the same collision-resistance role natively).
fn response_hash(url: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Host directory component (reference crawler `getResponseHost`).
fn response_host(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| u.host_str().unwrap_or("").replace(':', "_"))
        .unwrap_or_default()
}

/// Build the raw request text from a navigation request.
pub fn format_raw_request(method: &str, url: &str, body: &str, headers: &std::collections::HashMap<String, String>) -> String {
    let parsed = url::Url::parse(url).ok();
    let path = parsed
        .as_ref()
        .map(|u| {
            let p = u.path();
            let q = u.query().map(|q| format!("?{q}")).unwrap_or_default();
            format!("{p}{q}")
        })
        .unwrap_or_else(|| url.to_string());
    let host = parsed.as_ref().map(|u| u.host_str().unwrap_or("")).unwrap_or("");
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    for (k, v) in headers {
        raw.push_str(&format!("{k}: {v}\r\n"));
    }
    raw.push_str("\r\n");
    if !body.is_empty() {
        raw.push_str(body);
    }
    raw
}

/// Build the raw response text (status line + headers + body).
pub fn format_raw_response(status_code: i32, headers: &std::collections::HashMap<String, String>, body: &str) -> String {
    let mut raw = format!("HTTP/1.1 {status_code}\r\n");
    for (k, v) in headers {
        raw.push_str(&format!("{k}: {v}\r\n"));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    raw
}

/// Store a result's raw request/response under `<dir>/<host>/<hash>.txt` and
/// append an index line (reference crawler `updateIndex` / `getResponseFile`).
pub fn store_response(result: &Result, dir: &str) -> Option<String> {
    let request = result.request.as_ref()?;
    let response = result.response.as_ref()?;
    let host = response_host(&request.url);
    if host.is_empty() {
        return None;
    }
    let host_dir = Path::new(dir).join(&host);
    let _ = std::fs::create_dir_all(&host_dir);
    let file = host_dir.join(format!("{}.txt", response_hash(&request.url)));

    let raw_req = format_raw_request(&request.method, &request.url, &request.body, &request.headers);
    let raw_resp = format_raw_response(response.status_code, &response.headers, &response.body);
    let content = format!("{}\n\n\n{}\n\n{}", request.url, raw_req, raw_resp);

    if let Ok(mut f) = OpenOptions::new().create(true).write(true).truncate(true).open(&file) {
        let _ = f.write_all(content.as_bytes());
    }

    // Update the index file.
    let index = Path::new(dir).join("index.txt");
    if let Ok(mut idx) = OpenOptions::new().create(true).append(true).open(&index) {
        let _ = writeln!(
            idx,
            "{} {} ({})",
            file.display(),
            request.url,
            response.status_code
        );
    }

    file.to_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::result::{Request, Response};

    #[test]
    fn test_store_response_layout() {
        let tmp = std::env::temp_dir().join("bc_test_resp");
        let _ = std::fs::create_dir_all(&tmp);
        let result = Result {
            timestamp: String::new(),
            request: Some(Request {
                method: "GET".into(),
                url: "https://store.test/page".into(),
                headers: [("Host".to_string(), "store.test".to_string())].into_iter().collect(),
                ..Default::default()
            }),
            response: Some(Response {
                status_code: 200,
                body: "hello".into(),
                ..Default::default()
            }),
            error: String::new(),
        };
        let path = store_response(&result, tmp.to_str().unwrap());
        assert!(path.is_some());
        let content = std::fs::read_to_string(path.unwrap()).unwrap();
        assert!(content.contains("GET /page HTTP/1.1"));
        assert!(content.contains("hello"));
        assert!(tmp.join("index.txt").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_raw_request_format() {
        let raw = format_raw_request(
            "POST",
            "https://x.com/submit?a=1",
            "p=1",
            &Default::default(),
        );
        assert!(raw.starts_with("POST /submit?a=1 HTTP/1.1\r\n"));
        assert!(raw.contains("Host: x.com"));
        assert!(raw.ends_with("p=1"));
    }
}
