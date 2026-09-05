//! Port of the reference crawler `pkg/output` — the standard writer handling screen output
//! (verbose decorations + colors), JSONL output (with field exclusion), custom
//! output templates, field selection, per-host field storage, raw response
//! storage, and the error log.

pub mod fields;
pub mod responses;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::types::options::Options;
use crate::types::result::{Request, Result};

/// Minimal leveled logger mirroring the reference crawler's gologger levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Silent = 0,
    Error = 1,
    Info = 2,
    Warning = 3,
    Debug = 4,
}

/// Process-wide output verbosity (set once from options).
static LOG_LEVEL: Mutex<LogLevel> = Mutex::new(LogLevel::Info);
static NO_COLORS: Mutex<bool> = Mutex::new(false);

/// Configure logging from options (reference crawler `ConfigureOutput`).
pub fn configure_output(options: &Options) {
    let level = if options.silent {
        LogLevel::Silent
    } else if options.debug {
        LogLevel::Debug
    } else if options.verbose {
        LogLevel::Warning
    } else {
        LogLevel::Info
    };
    *LOG_LEVEL.lock().unwrap() = level;
    *NO_COLORS.lock().unwrap() = options.no_colors;
}

pub fn log(level: LogLevel, msg: &str) {
    let current = *LOG_LEVEL.lock().unwrap();
    if level > current {
        return;
    }
    match level {
        LogLevel::Debug => eprintln!("[DEBUG] {msg}"),
        LogLevel::Info => eprintln!("[INF] {msg}"),
        LogLevel::Warning => eprintln!("[WRN] {msg}"),
        LogLevel::Error => eprintln!("[ERR] {msg}"),
        LogLevel::Silent => {}
    }
}

fn colors_enabled() -> bool {
    !*NO_COLORS.lock().unwrap()
}

fn blue(s: &str) -> String {
    if colors_enabled() { format!("\x1b[34m{s}\x1b[0m") } else { s.to_string() }
}
fn green(s: &str) -> String {
    if colors_enabled() { format!("\x1b[32m{s}\x1b[0m") } else { s.to_string() }
}
fn yellow(s: &str) -> String {
    if colors_enabled() { format!("\x1b[33m{s}\x1b[0m") } else { s.to_string() }
}

/// The standard output writer (reference crawler `output.StandardWriter`).
pub struct StandardWriter {
    json: bool,
    verbose: bool,
    fields: String,
    store_fields: String,
    store_field_dir: String,
    store_response: bool,
    store_response_dir: String,
    omit_raw: bool,
    omit_body: bool,
    exclude_output_fields: Vec<String>,
    output_template: String,
    output_file: Option<Mutex<File>>,
    error_log: Option<Mutex<File>>,
}

impl StandardWriter {
    /// Build a writer from the validated options.
    pub fn from_options(options: &Options) -> StandardWriter {
        configure_output(options);

        let output_file = if options.output_file.is_empty() {
            None
        } else {
            let path = if options.no_clobber {
                non_clobber_path(Path::new(&options.output_file))
            } else {
                PathBuf::from(&options.output_file)
            };
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
                .map(Mutex::new)
        };

        let error_log = if options.error_log.is_empty() {
            None
        } else {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&options.error_log)
                .ok()
                .map(Mutex::new)
        };

        let store_response_dir = if options.store_response_dir.is_empty() {
            "output".to_string()
        } else {
            options.store_response_dir.clone()
        };
        if options.store_response {
            let _ = std::fs::create_dir_all(&store_response_dir);
        }

        let store_field_dir = if options.store_field_dir.is_empty() {
            "fields".to_string()
        } else {
            options.store_field_dir.clone()
        };
        if !options.store_fields.is_empty() {
            let _ = std::fs::create_dir_all(&store_field_dir);
        }

        StandardWriter {
            json: options.json,
            verbose: options.verbose,
            fields: options.fields.clone(),
            store_fields: options.store_fields.clone(),
            store_field_dir,
            store_response: options.store_response,
            store_response_dir,
            omit_raw: options.omit_raw,
            omit_body: options.omit_body,
            exclude_output_fields: options.exclude_output_fields.clone(),
            output_template: options.output_template.clone(),
            output_file,
            error_log,
        }
    }

    /// Write a result to the configured destinations (reference crawler `StandardWriter.Write`).
    pub fn write(&self, result: &mut Result) {
        // Apply output omissions (-or/-ob) before formatting.
        if self.omit_raw {
            if let Some(req) = result.request.as_mut() {
                req.raw.clear();
            }
            if let Some(resp) = result.response.as_mut() {
                resp.raw.clear();
            }
        }
        if self.omit_body {
            if let Some(resp) = result.response.as_mut() {
                resp.body.clear();
            }
        }

        // Attach stored response path when raw storage is on.
        if self.store_response {
            if let Some(path) = responses::store_response(result, &self.store_response_dir) {
                if let Some(resp) = result.response.as_mut() {
                    resp.stored_response_path = path;
                }
            }
        }

        if !self.store_fields.is_empty() {
            fields::store_fields(result, &self.store_fields, &self.store_field_dir);
        }

        let formatted = if self.json {
            self.format_json(result)
        } else if !self.output_template.is_empty() {
            self.format_template(result)
        } else if !self.fields.is_empty() {
            let mut builder = String::new();
            for fop in fields::format_field(result, &self.fields) {
                if self.verbose {
                    builder.push('[');
                    builder.push_str(&blue(&fop.field));
                    builder.push(']');
                    builder.push(' ');
                }
                builder.push_str(&fop.value);
                builder.push('\n');
            }
            builder
        } else {
            self.format_screen(result)
        };

        // Results always print to stdout; `-silent` only silences log lines.
        if !formatted.is_empty() {
            print!("{formatted}");
            let _ = std::io::stdout().flush();
        }

        if let Some(file) = &self.output_file {
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(formatted.as_bytes());
                let _ = f.flush();
            }
        }
    }

    /// Screen format: `[tag][method] url [body] [depth:x]` (reference crawler `formatScreen`).
    fn format_screen(&self, result: &Result) -> String {
        let Some(request) = &result.request else {
            return String::new();
        };
        let mut builder = String::new();

        if self.verbose && !request.tag.is_empty() {
            builder.push('[');
            builder.push_str(&blue(&request.tag));
            builder.push(']');
            builder.push(' ');
        }

        if !request.method.is_empty() && self.verbose {
            builder.push('[');
            builder.push_str(&green(&request.method));
            builder.push(']');
            builder.push(' ');
        }

        builder.push_str(&request.url);

        if !request.body.is_empty() && self.verbose {
            builder.push(' ');
            builder.push('[');
            builder.push_str(&request.body);
            builder.push(']');
        }

        if self.verbose {
            builder.push(' ');
            builder.push('[');
            builder.push_str(&yellow(&format!("depth:{}", request.depth)));
            builder.push(']');
        }

        builder.push('\n');
        builder
    }

    /// JSONL format with excluded fields (reference crawler `formatJSON`).
    fn format_json(&self, result: &Result) -> String {
        let mut value = match serde_json::to_value(result) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        apply_exclusions(&mut value, &self.exclude_output_fields);
        // Drop empty request/response objects like the reference crawler does.
        if let Some(req) = value.get_mut("request") {
            if req.as_object().map(|o| o.is_empty()).unwrap_or(false) {
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("request");
                }
            }
        }
        if let Some(resp) = value.get_mut("response") {
            if resp.as_object().map(|o| o.is_empty()).unwrap_or(false) {
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("response");
                }
            }
        }
        match serde_json::to_string(&value) {
            Ok(mut line) => {
                line.push('\n');
                line
            }
            Err(_) => String::new(),
        }
    }

    /// Custom output template: `{{path.to.field}}` substitution over the
    /// result's serialized JSON (native approximation of reference crawler `-ot`
    /// text/template with `{{formatRequest .Request}}`).
    fn format_template(&self, result: &Result) -> String {
        let mut value = match serde_json::to_value(result) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        apply_exclusions(&mut value, &self.exclude_output_fields);

        let mut out = self.output_template.clone();
        // Replace {{formatRequest .Request}} and generic {{.path}} tokens.
        while let Some(start) = out.find("{{") {
            let Some(end_rel) = out[start..].find("}}") else { break };
            let end = start + end_rel + 2;
            let token = out[start + 2..end - 2].trim();
            let replacement = if let Some(path) = token.strip_prefix('.') {
                lookup_json(&value, path.trim_start_matches('.'))
            } else {
                String::new()
            };
            out.replace_range(start..end, &replacement);
        }
        out.push('\n');
        out
    }

    /// Write a request error line to the error log (reference crawler `-elog`).
    pub fn write_error(&self, url: &str, error: &str) {
        if let Some(file) = &self.error_log {
            if let Ok(mut f) = file.lock() {
                let _ = writeln!(f, "{url} {error}");
            }
        }
    }
}

fn apply_exclusions(value: &mut Value, excluded: &[String]) {
    if excluded.is_empty() {
        return;
    }
    if let Some(obj) = value.as_object_mut() {
        for key in excluded {
            // Top-level keys and dotted paths into request/response.
            if let Some((head, rest)) = key.split_once('.') {
                if let Some(child) = obj.get_mut(head) {
                    apply_exclusions(child, &[rest.to_string()]);
                }
            } else {
                obj.remove(key);
            }
        }
    }
}

fn lookup_json(value: &Value, path: &str) -> String {
    let mut current = value;
    for part in path.split('.') {
        match current {
            Value::Object(map) => match map.get(part) {
                Some(v) => current = v,
                None => return String::new(),
            },
            _ => return String::new(),
        }
    }
    match current {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Produce a non-clobbering path like the reference crawler: `out.txt` -> `out-1.txt`, etc.
fn non_clobber_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());
    let parent = path.parent().unwrap_or(Path::new("."));
    for i in 1..1000 {
        let candidate = match &ext {
            Some(e) => parent.join(format!("{stem}-{i}.{e}")),
            None => parent.join(format!("{stem}-{i}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

/// Format the URL list line for `-lof` (list output fields).
pub fn list_output_fields() {
    println!("{}", fields::FIELD_NAMES.join("\n"));
}

/// Build a raw request line for a navigation request (used by responses.rs and
/// headless engine).
pub fn raw_request_of(request: &Request) -> String {
    responses::format_raw_request(&request.method, &request.url, &request.body, &request.headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::result::{Request, Response};

    fn sample() -> Result {
        Result {
            timestamp: "2026-01-01T00:00:00Z".into(),
            request: Some(Request {
                method: "GET".into(),
                url: "https://example.com/".into(),
                tag: "a".into(),
                attribute: "href".into(),
                depth: 2,
                ..Default::default()
            }),
            response: Some(Response {
                status_code: 200,
                body: "<html></html>".into(),
                ..Default::default()
            }),
            error: String::new(),
        }
    }

    #[test]
    fn test_format_screen_default() {
        let mut o = Options::with_defaults();
        o.silent = true;
        let w = StandardWriter::from_options(&o);
        let line = w.format_screen(&sample());
        assert_eq!(line.trim_end(), "https://example.com/");
    }

    #[test]
    fn test_format_screen_verbose() {
        let mut o = Options::with_defaults();
        o.silent = true;
        o.verbose = true;
        o.no_colors = true;
        let w = StandardWriter::from_options(&o);
        let line = w.format_screen(&sample());
        assert!(line.contains("[a]"));
        assert!(line.contains("[GET]"));
        assert!(line.contains("[depth:2]"));
    }

    #[test]
    fn test_format_json_exclusion() {
        let mut o = Options::with_defaults();
        o.silent = true;
        o.json = true;
        o.exclude_output_fields = vec!["timestamp".into(), "response.body".into()];
        let w = StandardWriter::from_options(&o);
        let line = w.format_json(&sample());
        assert!(!line.contains("timestamp"));
        assert!(!line.contains("__html"));
        assert!(line.contains("status_code"));
    }

    #[test]
    fn test_format_template() {
        let mut o = Options::with_defaults();
        o.silent = true;
        o.output_template = "{{.request.endpoint}} => {{.response.status_code}}".into();
        let w = StandardWriter::from_options(&o);
        let line = w.format_template(&sample());
        assert!(line.contains("https://example.com/ => 200"), "{line}");
    }

    #[test]
    fn test_non_clobber_path() {
        let tmp = std::env::temp_dir().join("bc_nc.txt");
        let _ = std::fs::write(&tmp, "x");
        let p = non_clobber_path(&tmp);
        assert_ne!(p, tmp);
        assert!(p.to_string_lossy().contains("bc_nc-1.txt"));
        let _ = std::fs::remove_file(&tmp);
    }
}
