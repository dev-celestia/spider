//! Technology detection (reference crawler `-td` uses `wappalyzergo`). Native Rust
//! approximation: fingerprint table over response headers and body markers.

use crate::types::result::{Headers, Response};

/// A single technology fingerprint: match against a header or body substring.
struct Fingerprint {
    name: &'static str,
    /// Header name (lowercased) to inspect; empty means body match.
    header: &'static str,
    /// Substring to match (case-insensitive for body, exact-ish for headers).
    pattern: &'static str,
}

static FINGERPRINTS: &[Fingerprint] = &[
    Fingerprint { name: "WordPress", header: "", pattern: "wp-content" },
    Fingerprint { name: "WordPress", header: "", pattern: "wp-includes" },
    Fingerprint { name: "Drupal", header: "", pattern: "drupal" },
    Fingerprint { name: "Joomla", header: "", pattern: "joomla" },
    Fingerprint { name: "Next.js", header: "", pattern: "__next" },
    Fingerprint { name: "Nuxt", header: "", pattern: "__nuxt" },
    Fingerprint { name: "Nuxt", header: "", pattern: "nuxt" },
    Fingerprint { name: "React", header: "", pattern: "react" },
    Fingerprint { name: "Vue.js", header: "", pattern: "vue" },
    Fingerprint { name: "Angular", header: "", pattern: "ng-version" },
    Fingerprint { name: "Svelte", header: "", pattern: "svelte" },
    Fingerprint { name: "jQuery", header: "", pattern: "jquery" },
    Fingerprint { name: "Bootstrap", header: "", pattern: "bootstrap" },
    Fingerprint { name: "Shopify", header: "", pattern: "shopify" },
    Fingerprint { name: "Cloudflare", header: "server", pattern: "cloudflare" },
    Fingerprint { name: "Nginx", header: "server", pattern: "nginx" },
    Fingerprint { name: "Apache", header: "server", pattern: "apache" },
    Fingerprint { name: "IIS", header: "server", pattern: "microsoft-iis" },
    Fingerprint { name: "LiteSpeed", header: "server", pattern: "litespeed" },
    Fingerprint { name: "Express", header: "x-powered-by", pattern: "express" },
    Fingerprint { name: "Next.js", header: "x-powered-by", pattern: "next.js" },
    Fingerprint { name: "PHP", header: "x-powered-by", pattern: "php" },
    Fingerprint { name: "ASP.NET", header: "x-powered-by", pattern: "asp.net" },
    Fingerprint { name: "ASP.NET", header: "", pattern: "__viewstate" },
    Fingerprint { name: "Django", header: "", pattern: "csrfmiddlewaretoken" },
    Fingerprint { name: "Laravel", header: "", pattern: "laravel_session" },
    Fingerprint { name: "Rails", header: "", pattern: "csrf-token" },
    Fingerprint { name: "Google Analytics", header: "", pattern: "googletagmanager" },
    Fingerprint { name: "Google Analytics", header: "", pattern: "google-analytics" },
    Fingerprint { name: "Vercel", header: "x-vercel-id", pattern: "" },
    Fingerprint { name: "Amazon CloudFront", header: "x-amz-cf-id", pattern: "" },
    Fingerprint { name: "GitHub Pages", header: "x-github-request-id", pattern: "" },
];

/// Detect technologies from a response (reference crawler `-td` native approximation).
pub fn detect_technologies(resp: &Response) -> Vec<String> {
    let mut found = Vec::new();
    let body_lower = resp.body.to_lowercase();
    for fp in FINGERPRINTS {
        let hit = if fp.header.is_empty() {
            body_lower.contains(&fp.pattern.to_lowercase())
        } else if !fp.pattern.is_empty() {
            resp.headers
                .iter()
                .any(|(k, v)| k.to_lowercase() == fp.header && v.to_lowercase().contains(&fp.pattern.to_lowercase()))
        } else {
            resp.headers.keys().any(|k| k.to_lowercase() == fp.header)
        };
        if hit && !found.iter().any(|f| f == fp.name) {
            found.push(fp.name.to_string());
        }
    }
    found
}

/// Flatten headers into lowercase-key map (reference crawler `utils.FlattenHeaders`).
pub fn flatten_headers(headers: &Headers) -> Headers {
    headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_from_body() {
        let mut r = Response::default();
        r.body = "<div id=\"__next\">app</div>".into();
        let techs = detect_technologies(&r);
        assert!(techs.contains(&"Next.js".to_string()));
    }

    #[test]
    fn test_detect_from_header() {
        let mut r = Response::default();
        r.headers.insert("Server".into(), "nginx/1.24".into());
        let techs = detect_technologies(&r);
        assert!(techs.contains(&"Nginx".to_string()));
    }

    #[test]
    fn test_header_presence_fingerprint() {
        let mut r = Response::default();
        r.headers.insert("X-Vercel-Id".into(), "cdg1::abc".into());
        let techs = detect_technologies(&r);
        assert!(techs.contains(&"Vercel".to_string()));
    }

    #[test]
    fn test_flatten_headers() {
        let mut h = Headers::new();
        h.insert("Content-Type".into(), "text/html".into());
        let flat = flatten_headers(&h);
        assert!(flat.contains_key("content-type"));
    }
}
