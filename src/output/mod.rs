//! Port of the reference crawler `pkg/output` — the standard writer handling screen output
//! (verbose decorations + colors), JSONL output (with field exclusion), custom
//! output templates, field selection, per-host field storage, raw response
//! storage, and the error log.

pub mod fields;
pub mod responses;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Configure logging from options (reference crawler `ConfigureOutput`:
/// Silent > Verbose > Debug > Info precedence).
pub fn configure_output(options: &Options) {
    let level = if options.silent {
        LogLevel::Silent
    } else if options.verbose {
        LogLevel::Warning
    } else if options.debug {
        LogLevel::Debug
    } else {
        LogLevel::Info
    };
    *LOG_LEVEL.lock().unwrap() = level;
}

pub fn log(level: LogLevel, msg: &str) {
    let current = *LOG_LEVEL.lock().unwrap();
    if level > current {
        return;
    }
    // Write instead of eprintln! so a closed stderr never panics.
    let line = match level {
        LogLevel::Debug => format!("[DEBUG] {msg}\n"),
        LogLevel::Info => format!("[INF] {msg}\n"),
        LogLevel::Warning => format!("[WRN] {msg}\n"),
        LogLevel::Error => format!("[ERR] {msg}\n"),
        LogLevel::Silent => return,
    };
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(line.as_bytes());
    let _ = err.flush();
}

/// Writer-scoped color wrappers (results must not depend on the global
/// logging color flag, which races between concurrent runs).
impl StandardWriter {
    fn w_blue(&self, s: &str) -> String {
        if !self.no_colors { format!("\x1b[34m{s}\x1b[0m") } else { s.to_string() }
    }
    fn w_green(&self, s: &str) -> String {
        if !self.no_colors { format!("\x1b[32m{s}\x1b[0m") } else { s.to_string() }
    }
    fn w_yellow(&self, s: &str) -> String {
        if !self.no_colors { format!("\x1b[33m{s}\x1b[0m") } else { s.to_string() }
    }
}

/// The standard output writer (reference crawler `output.StandardWriter`).
/// Output-time filtering (extension match, `-mr`/`-fr` regexes, `-mdc`/`-fdc`
/// DSL conditions, `-fpt` page type) mirrors the reference `Write()` pipeline.
pub struct StandardWriter {
    json: bool,
    verbose: bool,
    no_colors: bool,
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
    // Output-time filter pipeline (reference crawler Write()).
    extension_validator: crate::utils::extensions::ExtensionValidator,
    match_regex: Vec<regex::Regex>,
    filter_regex: Vec<regex::Regex>,
    output_match_condition: String,
    output_filter_condition: String,
    filter_page_type: Vec<String>,
    result_count: AtomicU64,
}

impl StandardWriter {
    /// Build a writer from the validated options.
    pub fn from_options(options: &Options) -> StandardWriter {
        configure_output(options);

        // The output file truncates on every run (reference crawler os.Create);
        // -ncb applies to the store-response directory, not the output file.
        let output_file = if options.output_file.is_empty() {
            None
        } else {
            File::create(&options.output_file)
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

        // Default store directories match the reference crawler naming
        // (celestia rebrand of katana_response / katana_field).
        let store_response_dir = if options.store_response_dir.is_empty() {
            "celestia_response".to_string()
        } else if options.no_clobber {
            non_clobber_dir(Path::new(&options.store_response_dir))
        } else {
            options.store_response_dir.clone()
        };
        if options.store_response {
            let _ = std::fs::create_dir_all(&store_response_dir);
            // Pre-create/truncate the response index on startup.
            let index = Path::new(&store_response_dir).join("index.txt");
            let _ = File::create(&index);
        }

        let store_field_dir = if options.store_field_dir.is_empty() {
            "celestia_field".to_string()
        } else {
            options.store_field_dir.clone()
        };
        if !options.store_fields.is_empty() {
            let _ = std::fs::create_dir_all(&store_field_dir);
        }

        StandardWriter {
            json: options.json,
            verbose: options.verbose,
            no_colors: options.no_colors,
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
            extension_validator: crate::utils::extensions::ExtensionValidator::new(
                &options.extensions_match,
                &options.extension_filter,
                options.no_default_ext_filter,
            ),
            match_regex: options.match_regex.clone(),
            filter_regex: options.filter_regex.clone(),
            output_match_condition: options.output_match_condition.clone(),
            output_filter_condition: options.output_filter_condition.clone(),
            filter_page_type: options.filter_page_type.clone(),
            result_count: AtomicU64::new(0),
        }
    }

    /// Number of results that produced output (reference crawler `resultCount`).
    pub fn result_count(&self) -> u64 {
        self.result_count.load(Ordering::SeqCst)
    }

    /// Write a result to the configured destinations (reference crawler
    /// `StandardWriter.Write`). Returns Err(reason) when the result was
    /// filtered out and produced no output.
    pub fn write(&self, result: &mut Result) -> std::result::Result<(), String> {
        let request_url = result
            .request
            .as_ref()
            .map(|r| r.url.clone())
            .unwrap_or_default();

        // Skip empty responses (e.g. from similarity filtering).
        if let Some(resp) = result.response.as_ref() {
            if resp.body.is_empty() && result.error.is_empty() {
                return Err("response filtered by similarity detection".to_string());
            }
        }

        if !self.store_fields.is_empty() {
            fields::store_fields(result, &self.store_fields, &self.store_field_dir);
        }

        if !self.extension_validator.validate_path(&request_url) {
            return Err("result does not match extension filter".to_string());
        }

        // matchOutput: regex then DSL condition (-mr / -mdc).
        if !self.match_regex.is_empty()
            && !self.match_regex.iter().any(|r| r.is_match(&request_url))
        {
            return Err("result does not match output".to_string());
        }
        if !self.output_match_condition.is_empty() {
            let ctx = self.dsl_context(result);
            if !crate::utils::dsl::eval_bool(&self.output_match_condition, &ctx).unwrap_or(false) {
                return Err("result does not match output".to_string());
            }
        }

        // filterOutput: regex then DSL condition (-fr / -fdc).
        if self.filter_regex.iter().any(|r| r.is_match(&request_url)) {
            return Err("result is filtered out".to_string());
        }
        if !self.output_filter_condition.is_empty() {
            let ctx = self.dsl_context(result);
            if crate::utils::dsl::eval_bool(&self.output_filter_condition, &ctx).unwrap_or(false) {
                return Err("result is filtered out".to_string());
            }
        }

        // Page-type filter (-fpt).
        if !self.filter_page_type.is_empty() {
            if let Some(resp) = result.response.as_ref() {
                if page_type_filtered(resp, &self.filter_page_type) {
                    return Err("result filtered by page type".to_string());
                }
            }
        }

        // Store the raw response BEFORE applying omissions (reference
        // crawler stores at output.go:214, omits at 234).
        if self.store_response {
            if let Some(path) = responses::store_response(result, &self.store_response_dir) {
                if let Some(resp) = result.response.as_mut() {
                    resp.stored_response_path = path;
                }
            }
        }

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

        // Format precedence: template > JSON > fields > screen
        // (reference crawler output.go:246-256).
        let formatted = if !self.output_template.is_empty() {
            self.format_template(result)
        } else if self.json {
            self.format_json(result)
        } else if !self.fields.is_empty() {
            let mut builder = String::new();
            for fop in fields::format_field(result, &self.fields) {
                if self.verbose {
                    builder.push('[');
                    builder.push_str(&self.w_blue(&fop.field));
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

        if formatted.is_empty() {
            return Err("result is empty".to_string());
        }

        // Increment result count only for valid results that produce output.
        self.result_count.fetch_add(1, Ordering::SeqCst);

        // Results always print to stdout; `-silent` only silences log lines.
        // Write (instead of print!) so a closed pipe (e.g. `| head`) returns
        // an error instead of panicking.
        {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(formatted.as_bytes());
            let _ = out.flush();
        }

        if let Some(file) = &self.output_file {
            // Non-JSON file output is decolorized (reference crawler decolorizerRegex).
            let data = if self.json {
                formatted.clone()
            } else {
                decolorize(&formatted)
            };
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(data.as_bytes());
                let _ = f.flush();
            }
        }

        Ok(())
    }

    /// JSON context for DSL output conditions.
    fn dsl_context(&self, result: &Result) -> serde_json::Value {
        match (&result.request, &result.response) {
            (Some(req), Some(resp)) => crate::engine::common::result_context(req, resp),
            (Some(req), None) => {
                serde_json::json!({
                    "url": req.url,
                    "method": req.method,
                    "tag": req.tag,
                    "attribute": req.attribute,
                    "source": req.source,
                    "depth": req.depth,
                })
            }
            _ => serde_json::json!({}),
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
            builder.push_str(&self.w_blue(&request.tag));
            builder.push(']');
            builder.push(' ');
        }

        if !request.method.is_empty() && self.verbose {
            builder.push('[');
            builder.push_str(&self.w_green(&request.method));
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
            builder.push_str(&self.w_yellow(&format!("depth:{}", request.depth)));
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

    /// Custom output template (reference crawler `-ot` fasttemplate): bare
    /// `{{token}}` names resolve against the 14 field selectors plus custom
    /// fields; an unknown tag drops the entire line. `{{.json.path}}` tokens
    /// resolve against the serialized result (Rust extension).
    fn format_template(&self, result: &Result) -> String {
        let mut value = match serde_json::to_value(result) {
            Ok(v) => v,
            Err(_) => return String::new(),
        };
        apply_exclusions(&mut value, &self.exclude_output_fields);

        // Field map: all 14 selectors + custom fields (reference crawler
        // formatTemplate uses formatField(FieldNames) + custom fields).
        let mut fields_map = std::collections::HashMap::new();
        for fo in fields::format_field(result, &fields::FIELD_NAMES.join(",")) {
            fields_map.insert(fo.field, fo.value);
        }
        if let Some(req) = &result.request {
            for (name, values) in &req.custom_fields {
                fields_map.insert(name.clone(), values.join(","));
            }
        }

        let mut out = self.output_template.clone();
        while let Some(start) = out.find("{{") {
            let Some(end_rel) = out[start..].find("}}") else { break };
            let end = start + end_rel + 2;
            let token = out[start + 2..end - 2].trim().to_string();
            let replacement = if let Some(path) = token.strip_prefix('.') {
                lookup_json(&value, path.trim_start_matches('.'))
            } else {
                match fields_map.get(&token) {
                    Some(v) => v.clone(),
                    // Unknown tag: the whole line is ignored.
                    None => return String::new(),
                }
            };
            out.replace_range(start..end, &replacement);
        }
        out.push('\n');
        out
    }

    /// Write a request error entry to the error log (reference crawler
    /// `WriteErr` marshals an `output.Error` JSON object).
    pub fn write_error(&self, url: &str, source: &str, error: &str) {
        let entry = serde_json::json!({
            "timestamp": crate::types::result::now_rfc3339(),
            "endpoint": url,
            "source": source,
            "error": error,
        });
        let data = serde_json::to_string(&entry).unwrap_or_default();
        if let Some(file) = &self.error_log {
            if let Ok(mut f) = file.lock() {
                let _ = writeln!(f, "{data}");
            }
        }
    }
}

/// Strip ANSI escape sequences (reference crawler `decolorizerRegex`).
fn decolorize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip CSI sequences: ESC [ ... final-byte letter.
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Page-type heuristics for `-fpt error,captcha,parked` (native approximation
/// of the reference crawler's dit page-type classifier).
pub fn page_type_filtered(response: &crate::types::result::Response, filter: &[String]) -> bool {
    if filter.is_empty() {
        return false;
    }
    let body = response.body.to_lowercase();
    for f in filter {
        match f.as_str() {
            "error" => {
                if response.status_code >= 400
                    || body.contains("page not found")
                    || body.contains("internal server error")
                {
                    return true;
                }
            }
            "captcha" => {
                if body.contains("recaptcha")
                    || body.contains("hcaptcha")
                    || body.contains("turnstile")
                {
                    return true;
                }
            }
            "parked" => {
                if body.contains("domain is for sale") || body.contains("buy this domain") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
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

/// Non-clobbering directory name for store-response dirs
/// (reference crawler `createDirNameNoClobber`: appends `-1`, `-2`, ...).
fn non_clobber_dir(path: &Path) -> String {
    non_clobber_path(path).to_string_lossy().to_string()
}

/// Deduplicate lines in every file under `dir` (reference crawler
/// `folderutil.DedupeLinesInFiles("katana_field")` run after crawling).
pub fn dedupe_lines_in_dir(dir: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = String::with_capacity(content.len());
        for line in content.lines() {
            if seen.insert(line.to_string()) {
                out.push_str(line);
                out.push('\n');
            }
        }
        if out != content {
            let _ = std::fs::write(&path, out);
        }
    }
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
    fn test_format_template_field_names() {
        let mut o = Options::with_defaults();
        o.silent = true;
        o.no_colors = true;
        o.output_template = "{{fqdn}}{{path}}".into();
        let w = StandardWriter::from_options(&o);
        let line = w.format_template(&sample());
        assert!(line.contains("example.com/"), "{line}");
    }

    #[test]
    fn test_format_template_unknown_tag_drops_line() {
        let mut o = Options::with_defaults();
        o.silent = true;
        o.output_template = "{{url}} {{bogus_tag}}".into();
        let w = StandardWriter::from_options(&o);
        let line = w.format_template(&sample());
        assert_eq!(line, "", "unknown tag drops the whole line");
    }

    #[test]
    fn test_dedupe_lines_in_dir() {
        let tmp = std::env::temp_dir().join("bc_dedupe_dir");
        let _ = std::fs::create_dir_all(&tmp);
        let f = tmp.join("host_field.txt");
        std::fs::write(&f, "a\nb\na\n").unwrap();
        dedupe_lines_in_dir(tmp.to_str().unwrap());
        let content = std::fs::read_to_string(&f).unwrap();
        assert_eq!(content, "a\nb\n");
        let _ = std::fs::remove_dir_all(&tmp);
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
