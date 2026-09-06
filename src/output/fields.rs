//! Port of the reference crawler `pkg/output/fields.go` — the 14 supported output field
//! selectors, per-field value extraction, and per-host field storage.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use url::Url;

use crate::types::result::Result;

/// Supported field names (reference crawler `output.FieldNames`).
pub const FIELD_NAMES: &[&str] = &[
    "url", "path", "fqdn", "rdn", "rurl", "qurl", "qpath", "file", "ufile", "key", "value", "kv",
    "dir", "udir",
];

/// Validate a comma-separated field list (reference crawler `validateFieldNames`).
pub fn validate_field_names(names: &str) -> std::result::Result<(), String> {
    let parts: Vec<&str> = names.split(',').collect();
    if parts.is_empty() {
        return Err(format!("customfield: no field names provided: {names}"));
    }
    for part in parts {
        if !FIELD_NAMES.contains(&part) {
            return Err(format!("customfield: invalid field {part} specified: {names}"));
        }
    }
    Ok(())
}

/// Effective top-level domain + 1 (the reference crawler uses `publicsuffix.EffectiveTLDPlusOne`).
pub fn etld_plus_one(hostname: &str) -> String {
    let lower = hostname.to_lowercase();
    if let Some(suffix) = psl::suffix(lower.as_bytes()) {
        let suffix_str = suffix.as_bytes();
        // psl returns the public suffix ("com", "co.uk"); strip it plus one label.
        if lower.len() > suffix_str.len() {
            let idx = lower.len() - suffix_str.len();
            if idx >= 1 && lower.as_bytes()[idx - 1] == b'.' {
                let without_suffix = &lower[..idx - 1];
                if let Some(dot) = without_suffix.rfind('.') {
                    return lower[dot + 1..].to_string();
                }
                return lower.clone();
            }
        }
        return lower;
    }
    lower
}

pub struct FieldValue {
    pub field: String,
    pub value: String,
}

/// Format output results based on the requested field list
/// (reference crawler `formatField`).
pub fn format_field(result: &Result, fields: &str) -> Vec<FieldValue> {
    let mut values = Vec::new();
    let Some(request) = &result.request else {
        return values;
    };
    let parsed = match Url::parse(&request.url) {
        Ok(p) => p,
        Err(_) => return values,
    };

    let query_keys: Vec<String> = parsed.query_pairs().map(|(k, _)| k.to_string()).collect();
    let query_values: Vec<String> = parsed.query_pairs().map(|(_, v)| v.to_string()).collect();
    let query_both: Vec<String> = parsed
        .query_pairs()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    let hostname = parsed.host_str().unwrap_or("").to_string();
    let rdn = etld_plus_one(&hostname);
    let host_with_port = host_authority(&parsed);
    let rurl = format!("{}://{}", parsed.scheme(), host_with_port);

    for f in fields.split(',') {
        match f {
            "url" => values.push(FieldValue {
                field: "url".into(),
                value: request.url.clone(),
            }),
            "rdn" => values.push(FieldValue { field: "rdn".into(), value: rdn.clone() }),
            "path" => {
                if !parsed.path().is_empty() {
                    values.push(FieldValue {
                        field: "path".into(),
                        value: parsed.path().to_string(),
                    });
                }
            }
            "fqdn" => values.push(FieldValue {
                field: "fqdn".into(),
                value: hostname.clone(),
            }),
            "rurl" => values.push(FieldValue { field: "rurl".into(), value: rurl.clone() }),
            "qpath" => {
                if !query_keys.is_empty() {
                    // Reference crawler re-encodes and sorts the query
                    // (url.Values.Encode): keys sorted, pairs re-encoded.
                    let mut pairs: Vec<(String, String)> = parsed
                        .query_pairs()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                    pairs.sort_by(|a, b| a.0.cmp(&b.0));
                    let query = pairs
                        .iter()
                        .map(|(k, v)| format!("{}={}", encode_query_component(k), encode_query_component(v)))
                        .collect::<Vec<_>>()
                        .join("&");
                    values.push(FieldValue {
                        field: "qpath".into(),
                        value: format!("{}?{}", parsed.path(), query),
                    });
                }
            }
            "qurl" => {
                if !query_keys.is_empty() {
                    values.push(FieldValue {
                        field: "qurl".into(),
                        value: request.url.clone(),
                    });
                }
            }
            "key" => {
                for k in &query_keys {
                    values.push(FieldValue { field: "key".into(), value: k.clone() });
                }
            }
            "kv" => {
                for kv in &query_both {
                    values.push(FieldValue { field: "kv".into(), value: kv.clone() });
                }
            }
            "value" => {
                for v in &query_values {
                    values.push(FieldValue { field: "value".into(), value: v.clone() });
                }
            }
            "file" => {
                if let Some(base) = file_base(parsed.path()) {
                    values.push(FieldValue { field: "file".into(), value: base });
                }
            }
            "ufile" => {
                if file_base(parsed.path()).is_some() {
                    values.push(FieldValue {
                        field: "ufile".into(),
                        value: request.url.clone(),
                    });
                }
            }
            "udir" => {
                if let Some(dir) = path_directory(parsed.path()) {
                    values.push(FieldValue {
                        field: "udir".into(),
                        value: format!("{rurl}{dir}"),
                    });
                }
            }
            "dir" => {
                if let Some(dir) = path_directory(parsed.path()) {
                    values.push(FieldValue { field: "dir".into(), value: dir });
                }
            }
            other => {
                if let Some(vs) = request.custom_fields.get(other) {
                    for v in vs {
                        values.push(FieldValue {
                            field: other.to_string(),
                            value: v.clone(),
                        });
                    }
                }
            }
        }
    }
    values
}

/// Host including the explicit port when present
/// (reference crawler `parsed.Host` keeps `:port`).
fn host_authority(parsed: &Url) -> String {
    match parsed.port() {
        Some(port) => format!("{}:{}", parsed.host_str().unwrap_or(""), port),
        None => parsed.host_str().unwrap_or("").to_string(),
    }
}

/// Percent-encode a query component like Go's `url.Values.Encode`.
fn encode_query_component(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

/// Return the base filename if the path contains a dotted file.
fn file_base(path: &str) -> Option<String> {
    if path.is_empty() || path == "/" {
        return None;
    }
    let base = Path::new(path).file_name()?.to_string_lossy().to_string();
    if base.contains('.') {
        Some(base)
    } else {
        None
    }
}

/// Return the directory part of the path if it has at least one slash after
/// the first character (reference crawler `dir` semantics).
fn path_directory(path: &str) -> Option<String> {
    if path.is_empty() || path == "/" {
        return None;
    }
    if let Some(idx) = path[1..].rfind('/') {
        Some(path[..idx + 2].to_string())
    } else {
        None
    }
}

/// Store field values for a result into individual per-host files
/// (reference crawler `storeFields`): `<dir>/<scheme>_<hostname>_<field>.txt`.
pub fn store_fields(result: &Result, fields: &str, dir: &str) {
    let Some(request) = &result.request else { return };
    let parsed = match Url::parse(&request.url) {
        Ok(p) => p,
        Err(_) => return,
    };
    // Host directory keeps the port (reference crawler uses u.Host with
    // `:` replaced by `_`).
    let hostname = host_authority(&parsed).replace(':', "_");

    for fv in format_field(result, fields) {
        append_to_field_file(dir, parsed.scheme(), &hostname, &fv.field, &fv.value);
    }
}

fn append_to_field_file(dir: &str, scheme: &str, hostname: &str, field: &str, data: &str) {
    let path = Path::new(dir).join(format!("{scheme}_{hostname}_{field}.txt"));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{data}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::result::{Request, Response};

    fn result_for(url: &str) -> Result {
        Result {
            timestamp: String::new(),
            request: Some(Request {
                method: "GET".into(),
                url: url.into(),
                ..Default::default()
            }),
            response: Some(Response::default()),
            error: String::new(),
        }
    }

    #[test]
    fn test_field_names_valid() {
        assert!(validate_field_names("url,path,fqdn").is_ok());
        assert!(validate_field_names("bogus").is_err());
    }

    #[test]
    fn test_etld_plus_one() {
        assert_eq!(etld_plus_one("sub.example.com"), "example.com");
        assert_eq!(etld_plus_one("a.b.co.uk"), "b.co.uk");
        assert_eq!(etld_plus_one("localhost"), "localhost");
    }

    #[test]
    fn test_format_field_url_fqdn_rdn() {
        let r = result_for("https://sub.example.com/a/b?q=1");
        let fvs = format_field(&r, "url,fqdn,rdn,rurl");
        let get = |f: &str| {
            fvs.iter()
                .find(|v| v.field == f)
                .map(|v| v.value.clone())
                .unwrap()
        };
        assert_eq!(get("url"), "https://sub.example.com/a/b?q=1");
        assert_eq!(get("fqdn"), "sub.example.com");
        assert_eq!(get("rdn"), "example.com");
        assert_eq!(get("rurl"), "https://sub.example.com");
    }

    #[test]
    fn test_format_field_query_key_value() {
        let r = result_for("https://x.com/p?a=1&b=2");
        let fvs = format_field(&r, "key,value,kv,qurl,qpath");
        let vals = |f: &str| -> Vec<String> {
            fvs.iter()
                .filter(|v| v.field == f)
                .map(|v| v.value.clone())
                .collect()
        };
        assert_eq!(vals("key"), vec!["a", "b"]);
        assert_eq!(vals("value"), vec!["1", "2"]);
        assert_eq!(vals("kv"), vec!["a=1", "b=2"]);
        assert_eq!(vals("qurl"), vec!["https://x.com/p?a=1&b=2"]);
        assert_eq!(vals("qpath"), vec!["/p?a=1&b=2"]);
    }

    #[test]
    fn test_format_field_file_dir() {
        let r = result_for("https://x.com/js/app.js");
        let fvs = format_field(&r, "file,ufile,dir,udir");
        let get = |f: &str| {
            fvs.iter()
                .find(|v| v.field == f)
                .map(|v| v.value.clone())
                .unwrap()
        };
        assert_eq!(get("file"), "app.js");
        assert_eq!(get("ufile"), "https://x.com/js/app.js");
        assert_eq!(get("dir"), "/js/");
        assert_eq!(get("udir"), "https://x.com/js/");
    }

    #[test]
    fn test_store_fields_writes_file() {
        let tmp = std::env::temp_dir().join("bc_test_fields");
        let _ = std::fs::create_dir_all(&tmp);
        let r = result_for("https://store.example.com/page");
        store_fields(&r, "fqdn", tmp.to_str().unwrap());
        let f = tmp.join("https_store.example.com_fqdn.txt");
        assert!(f.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
