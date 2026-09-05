//! Port of the reference crawler `pkg/knowledgebase` — native approximation of the knowledge
//! base: secrets extraction (Titus-style regex fingerprints) and endpoint
//! classification (REST/GraphQL/SOAP/XHR).

use crate::types::result::Response;

static SECRET_PATTERNS: &[(&str, &str)] = &[
    ("AWS Access Key", r"AKIA[0-9A-Z]{16}"),
    ("AWS Secret Key", r#"(?i)aws(.{0,20})?(secret|sk)(.{0,20})?['"][0-9a-zA-Z/+]{40}['"]"#),
    ("Google API Key", r"AIza[0-9A-Za-z\\-_]{35}"),
    ("Google OAuth", r"[0-9]+-[0-9A-Za-z_]{32}\.apps\.googleusercontent\.com"),
    ("Slack Token", r"xox[baprs]-[0-9a-zA-Z]{10,48}"),
    ("GitHub Token", r"gh[pousr]_[0-9A-Za-z]{36}"),
    ("JWT", r"eyJ[A-Za-z0-9\-_]{10,}\.[A-Za-z0-9\-_]{10,}\.[A-Za-z0-9\-_]{5,}"),
    ("Stripe Key", r"(sk|pk)_(test|live)_[0-9a-zA-Z]{10,32}"),
    ("Twilio Key", r"SK[0-9a-fA-F]{32}"),
    ("Mailgun Key", r"key-[0-9a-zA-Z]{32}"),
    ("Firebase", r"AAAA[A-Za-z0-9_-]{7}:[A-Za-z0-9_-]{140}"),
    ("Private Key", r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----"),
    ("Authorization Bearer", r#"(?i)authorization['\"]?\s*[:=]\s*['\"]?Bearer\s+[A-Za-z0-9\-_.=+/]{20,}"#),
];

static ENDPOINT_HINTS: &[(&str, &str)] = &[
    ("/api/", "rest"),
    ("/rest/", "rest"),
    ("/graphql", "graphql"),
    ("/query", "graphql"),
    (".asmx", "soap"),
    ("/wsdl", "soap"),
    ("/xhr/", "xhr"),
    ("/ajax/", "xhr"),
];

/// Analyze a response for secrets and classified endpoints, returning the
/// `knowledgebase` JSON attached to output results (reference crawler `-kb`).
pub fn analyze(response: &Response) -> serde_json::Value {
    let mut result = serde_json::Map::new();

    let secrets = extract_secrets(&response.body);
    if !secrets.is_empty() {
        result.insert(
            "secrets".into(),
            serde_json::Value::Array(
                secrets
                    .into_iter()
                    .map(|(kind, _match)| {
                        serde_json::json!({"type": kind, "match": "[REDACTED]"})
                    })
                    .collect(),
            ),
        );
    }

    let endpoints = classify_endpoints(&response.body);
    if !endpoints.is_empty() {
        result.insert(
            "endpoints".into(),
            serde_json::Value::Array(
                endpoints
                    .into_iter()
                    .map(|(kind, path)| serde_json::json!({"type": kind, "path": path}))
                    .collect(),
            ),
        );
    }

    serde_json::Value::Object(result)
}

/// Extract (kind, match) pairs from content (reference crawler `-kb-secrets`).
pub fn extract_secrets(content: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (name, pattern) in SECRET_PATTERNS {
        if let Ok(re) = regex::Regex::new(pattern) {
            for m in re.find_iter(content).take(10) {
                found.push((name.to_string(), m.as_str().to_string()));
            }
        }
    }
    found
}

/// Classify endpoint-looking paths in content (reference crawler `-kb-endpoints`).
pub fn classify_endpoints(content: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (hint, kind) in ENDPOINT_HINTS {
        for m in content.split(|c: char| !c.is_ascii_graphic()) {
            if m.contains(hint) && m.len() < 200 {
                let key = (kind.to_string(), m.to_string());
                if !found.contains(&key) {
                    found.push(key);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_secrets_aws() {
        let content = "key = AKIAIOSFODNN7EXAMPLE";
        let found = extract_secrets(content);
        assert!(found.iter().any(|(k, _)| k == "AWS Access Key"));
    }

    #[test]
    fn test_extract_secrets_jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let found = extract_secrets(jwt);
        assert!(found.iter().any(|(k, _)| k == "JWT"), "{found:?}");
    }

    #[test]
    fn test_classify_endpoints() {
        let content = r#"fetch("/api/v1/users"); url = "/graphql";"#;
        let found = classify_endpoints(content);
        assert!(found.iter().any(|(k, _)| k == "rest"));
        assert!(found.iter().any(|(k, _)| k == "graphql"));
    }

    #[test]
    fn test_analyze_shape() {
        let mut r = Response::default();
        r.body = "AKIAIOSFODNN7EXAMPLE and /api/users".into();
        let v = analyze(&r);
        assert!(v.get("secrets").is_some());
        assert!(v.get("endpoints").is_some());
    }

    #[test]
    fn test_clean_content_no_secrets() {
        assert!(extract_secrets("hello world").is_empty());
    }
}
