//! Port of the reference crawler `pkg/engine/parser` — response parsers extracting navigation
//! requests from headers (Content-Location, Link, Refresh, Location) and body
//! (all celestia tag/attribute parsers), plus JS endpoint scraping and form
//! extraction.

use scraper::{Html, Selector};

use crate::types::options::Options;
use crate::types::result::{Form, Request, Response};
use crate::utils::regex as uregex;

/// Options controlling which parsers are active (reference crawler `parser.InitWithOptions`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ParserOptions {
    pub automatic_form_fill: bool,
    pub scrape_js_responses: bool,
    pub scrape_jsluice_responses: bool,
    pub disable_redirects: bool,
    pub form_extraction: bool,
}

impl ParserOptions {
    pub fn from_options(options: &Options) -> Self {
        ParserOptions {
            automatic_form_fill: options.automatic_form_fill,
            scrape_js_responses: options.scrape_js_responses,
            scrape_jsluice_responses: options.scrape_jsluice_responses,
            disable_redirects: options.disable_redirects,
            form_extraction: options.form_extraction,
        }
    }
}

/// A single (tag, attribute) extraction rule — the common shape of most celestia
/// body parsers.
struct TagAttr {
    selector: &'static str,
    tag: &'static str,
    attribute: &'static str,
}

const SIMPLE_TAG_ATTRS: &[TagAttr] = &[
    TagAttr { selector: "a[href]", tag: "a", attribute: "href" },
    TagAttr { selector: "a[ping]", tag: "a", attribute: "ping" },
    TagAttr { selector: "link[href]", tag: "link", attribute: "href" },
    TagAttr { selector: "embed[src]", tag: "embed", attribute: "src" },
    TagAttr { selector: "frame[src]", tag: "frame", attribute: "src" },
    TagAttr { selector: "iframe[src]", tag: "iframe", attribute: "src" },
    TagAttr { selector: "input[type='image'][src]", tag: "input-image", attribute: "src" },
    TagAttr { selector: "isindex[action]", tag: "isindex", attribute: "action" },
    TagAttr { selector: "script[src]", tag: "script", attribute: "src" },
    TagAttr { selector: "body[background]", tag: "body", attribute: "background" },
    TagAttr { selector: "applet[archive]", tag: "applet", attribute: "archive" },
    TagAttr { selector: "applet[codebase]", tag: "applet", attribute: "codebase" },
    TagAttr { selector: "blockquote[cite]", tag: "blockquote", attribute: "cite" },
    TagAttr { selector: "area[ping]", tag: "area", attribute: "ping" },
    TagAttr { selector: "base[href]", tag: "base", attribute: "href" },
    TagAttr { selector: "import[implementation]", tag: "import", attribute: "implementation" },
    TagAttr { selector: "button[formaction]", tag: "button", attribute: "formaction" },
    TagAttr { selector: "html[manifest]", tag: "html", attribute: "manifest" },
    TagAttr { selector: "table[background]", tag: "table", attribute: "background" },
    TagAttr { selector: "td[background]", tag: "table", attribute: "td-background" },
    TagAttr { selector: "video[src]", tag: "video", attribute: "src" },
    TagAttr { selector: "video[poster]", tag: "video", attribute: "poster" },
    TagAttr { selector: "video track[src]", tag: "video", attribute: "track-src" },
    TagAttr { selector: "audio[src]", tag: "audio", attribute: "src" },
    TagAttr { selector: "audio source[src]", tag: "audio", attribute: "source" },
    TagAttr { selector: "svg image[href]", tag: "svg", attribute: "image-href" },
    TagAttr { selector: "svg script[href]", tag: "svg", attribute: "script-href" },
    TagAttr { selector: "object[data]", tag: "src", attribute: "data" },
    TagAttr { selector: "object[codebase]", tag: "src", attribute: "codebase" },
    TagAttr { selector: "object param[value]", tag: "src", attribute: "value" },
];

/// Extract valid navigation requests from parsed HTML (reference crawler `Parser.ParseResponse`).
pub fn parse_html(resp: &Response, document: &Html, opts: &ParserOptions) -> Vec<Request> {
    let mut requests = Vec::new();
    let source = resp.source.clone();

    // Header parsers first.
    requests.extend(parse_headers(resp, opts));

    // Simple tag/attribute extractions.
    for rule in SIMPLE_TAG_ATTRS {
        if let Ok(selector) = Selector::parse(rule.selector) {
            for element in document.select(&selector) {
                if let Some(value) = element.value().attr(rule.attribute) {
                    if !value.is_empty() {
                        requests.push(Request::from_response(
                            value,
                            &source,
                            rule.tag,
                            rule.attribute,
                            resp,
                        ));
                    }
                }
            }
        }
    }

    // img tag: dynsrc, longdesc, lowsrc, src (skipping data:), srcset.
    if let Ok(sel) = Selector::parse("img") {
        for element in document.select(&sel) {
            for attr in ["dynsrc", "longdesc", "lowsrc"] {
                if let Some(v) = element.value().attr(attr) {
                    if !v.is_empty() {
                        requests.push(Request::from_response(v, &source, "img", attr, resp));
                    }
                }
            }
            if let Some(src) = element.value().attr("src") {
                if !src.is_empty() && src != "#" && !src.starts_with("data:") {
                    requests.push(Request::from_response(src, &source, "img", "src", resp));
                }
            }
            if let Some(srcset) = element.value().attr("srcset") {
                for value in uregex::parse_srcset_tag(srcset) {
                    requests.push(Request::from_response(
                        &value,
                        &source,
                        "img",
                        "srcset",
                        resp,
                    ));
                }
            }
        }
    }

    // audio source srcset.
    if let Ok(sel) = Selector::parse("audio source[srcset]") {
        for element in document.select(&sel) {
            if let Some(srcset) = element.value().attr("srcset") {
                for value in uregex::parse_srcset_tag(srcset) {
                    requests.push(Request::from_response(
                        &value,
                        &source,
                        "audio",
                        "sourcesrcset",
                        resp,
                    ));
                }
            }
        }
    }

    // iframe srcdoc: relative endpoint extraction from inline content.
    if let Ok(sel) = Selector::parse("iframe[srcdoc]") {
        for element in document.select(&sel) {
            if let Some(srcdoc) = element.value().attr("srcdoc") {
                if !srcdoc.is_empty() {
                    for endpoint in uregex::extract_relative_endpoints(srcdoc) {
                        requests.push(Request::from_response(
                            &endpoint,
                            &source,
                            "iframe",
                            "srcdoc",
                            resp,
                        ));
                    }
                }
            }
        }
    }

    // meta refresh / content URLs.
    if let Ok(sel) = Selector::parse("meta[content]") {
        for element in document.select(&sel) {
            if let Some(content) = element.value().attr("content") {
                for endpoint in uregex::extract_relative_endpoints(content) {
                    requests.push(Request::from_response(
                        &endpoint,
                        &source,
                        "meta",
                        "refresh",
                        resp,
                    ));
                }
            }
        }
    }

    // htmx attributes (hx-get/hx-post/hx-put/hx-patch; hx-delete excluded like the reference crawler).
    for (attr, method) in [
        ("hx-get", "GET"),
        ("hx-post", "POST"),
        ("hx-put", "PUT"),
        ("hx-patch", "PATCH"),
    ] {
        if let Ok(sel) = Selector::parse(&format!("[{attr}]")) {
            for element in document.select(&sel) {
                if let Some(value) = element.value().attr(attr) {
                    if value.is_empty() {
                        continue;
                    }
                    let mut req = Request::from_response(value, &source, "htmx", attr, resp);
                    req.method = method.to_string();
                    requests.push(req);
                }
            }
        }
    }

    // Doctype SYSTEM url (raw regex over the body — scraper drops doctype data).
    for endpoint in extract_doctype_system(&resp.body) {
        requests.push(Request::from_response(&endpoint, &source, "html", "doctype", resp));
    }

    // Custom field regex parser + JS scraping.
    if opts.scrape_js_responses {
        requests.extend(parse_script_contents(resp, document));
    }
    if opts.scrape_jsluice_responses {
        requests.extend(parse_script_contents_jsluice(resp, document));
    }

    // Form extraction metadata (attached to the response by the caller).
    if opts.form_extraction {
        // handled via extract_forms(); callers attach to the response.
    }

    requests.retain(|r| is_valid_navigation_request(r));
    requests
}

/// Parse form elements into metadata (reference crawler `-fx` form extraction).
pub fn extract_forms(document: &Html) -> Vec<Form> {
    let mut forms = Vec::new();
    let Ok(sel) = Selector::parse("form") else {
        return forms;
    };
    for form in document.select(&sel) {
        let method = form
            .value()
            .attr("method")
            .unwrap_or("GET")
            .to_uppercase();
        let action = form.value().attr("action").unwrap_or("").to_string();
        let enctype = form
            .value()
            .attr("enctype")
            .unwrap_or("application/x-www-form-urlencoded")
            .to_string();
        let mut parameters = Vec::new();
        if let Ok(input_sel) = Selector::parse("input, select, textarea") {
            for input in form.select(&input_sel) {
                if let Some(name) = input.value().attr("name") {
                    parameters.push(name.to_string());
                }
            }
        }
        forms.push(Form { method, action, enctype, parameters });
    }
    forms
}

/// Header parsers: Content-Location, Link, Refresh, and Location (redirects).
fn parse_headers(resp: &Response, opts: &ParserOptions) -> Vec<Request> {
    let mut requests = Vec::new();
    let source = resp.source.clone();
    let header = |name: &str| -> String {
        resp.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };

    let content_location = header("Content-Location");
    if !content_location.is_empty() {
        requests.push(Request::from_response(
            &content_location,
            &source,
            "header",
            "content-location",
            resp,
        ));
    }

    let link = header("Link");
    if !link.is_empty() {
        for value in uregex::parse_link_tag(&link) {
            requests.push(Request::from_response(&value, &source, "header", "link", resp));
        }
    }

    let refresh = header("Refresh");
    if !refresh.is_empty() {
        let value = uregex::parse_refresh_tag(&refresh);
        if !value.is_empty() {
            requests.push(Request::from_response(&value, &source, "header", "refresh", resp));
        }
    }

    if !opts.disable_redirects {
        let location = header("Location");
        if !location.is_empty() {
            requests.push(Request::from_response(&location, &source, "header", "location", resp));
        }
    }

    requests
}

/// Extract endpoint URLs from script tag contents (reference crawler `scriptContentRegexParser`).
fn parse_script_contents(resp: &Response, document: &Html) -> Vec<Request> {
    let mut requests = Vec::new();
    let source = resp.source.clone();
    if let Ok(sel) = Selector::parse("script") {
        for element in document.select(&sel) {
            let text = element.text().collect::<String>();
            if text.is_empty() {
                continue;
            }
            for endpoint in uregex::extract_relative_endpoints(&text) {
                requests.push(Request::from_response(
                    &endpoint,
                    &source,
                    "script",
                    "text",
                    resp,
                ));
            }
        }
    }
    requests
}

/// Extract endpoints from inline scripts using the jsluice-style extractor
/// (reference crawler `scriptContentJsluiceParser`, native approximation).
fn parse_script_contents_jsluice(resp: &Response, document: &Html) -> Vec<Request> {
    let mut requests = Vec::new();
    let source = resp.source.clone();
    if let Ok(sel) = Selector::parse("script") {
        for element in document.select(&sel) {
            let text = element.text().collect::<String>();
            if text.is_empty() {
                continue;
            }
            for endpoint in jsluice_endpoints(&text) {
                requests.push(Request::from_response(
                    &endpoint,
                    &source,
                    "script",
                    "jsluice-url",
                    resp,
                ));
            }
        }
    }
    requests
}

/// Parse a standalone JS/CSS file body for endpoints (celestia
/// `scriptJSFileRegexParser` / `scriptJSFileJsluiceParser`).
pub fn parse_js_file(resp: &Response, opts: &ParserOptions) -> Vec<Request> {
    let content_type = resp
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let path = url::Url::parse(&resp.source)
        .map(|u| u.path().to_string())
        .unwrap_or_default();
    let is_js = path.ends_with(".js") || path.ends_with(".css") || content_type.contains("/javascript");
    if !is_js || is_common_js_library(&path) {
        return Vec::new();
    }

    let tag = if opts.scrape_jsluice_responses { "js" } else { "js" };
    let extract = if opts.scrape_jsluice_responses {
        jsluice_endpoints(&resp.body)
    } else {
        uregex::extract_relative_endpoints(&resp.body)
    };
    let mut requests = Vec::new();
    for endpoint in extract {
        requests.push(Request::from_response(&endpoint, &resp.source, tag, "text", resp));
    }
    requests
}

/// Common JS library files are skipped for endpoint scraping
/// (reference crawler `IsPathCommonJSLibraryFile`).
pub fn is_common_js_library(path: &str) -> bool {
    let lower = path.to_lowercase();
    [
        "jquery", "angular", "bootstrap", "vue", "react", "lodash", "moment", "d3", "three",
        "socket.io", "backbone", "underscore", "modernizr", "polyfill",
    ]
    .iter()
    .any(|lib| lower.contains(lib))
}

/// jsluice-style endpoint extraction: URL-shaped strings in JS source
/// (native approximation of reference crawler `-jsl`).
pub fn jsluice_endpoints(content: &str) -> Vec<String> {
    uregex::extract_relative_endpoints(content)
}

/// Extract `SYSTEM "url"` from a doctype declaration.
fn extract_doctype_system(body: &str) -> Vec<String> {
    let lower_head = body
        .get(..body.len().min(2048))
        .unwrap_or("")
        .to_lowercase();
    let Some(idx) = lower_head.find("<!doctype") else { return Vec::new() };
    let Some(end) = body[idx..].find('>') else { return Vec::new() };
    let doctype = &body[idx..idx + end];
    let lower = doctype.to_lowercase();
    let Some(sys_idx) = lower.find("system") else { return Vec::new() };
    let rest = &doctype[sys_idx + "system".len()..];
    let rest_trim = rest.trim_start();
    if rest_trim.is_empty() {
        return Vec::new();
    }
    let quote = rest_trim.chars().next().unwrap();
    if quote != '"' && quote != '\'' {
        return Vec::new();
    }
    match rest_trim[1..].find(quote) {
        Some(end) => vec![rest_trim[1..1 + end].to_string()],
        None => Vec::new(),
    }
}

/// Reject empty/data/mailto/javascript/vbscript navigations
/// (reference crawler `isValidNavigationRequest`).
pub fn is_valid_navigation_request(req: &Request) -> bool {
    let url = req.url.trim();
    if url.is_empty() {
        return false;
    }
    let lc = url.to_lowercase();
    !(lc.starts_with("data:")
        || lc.starts_with("mailto:")
        || lc.starts_with("javascript:")
        || lc.starts_with("vbscript:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(base: &str, body: &str) -> (Response, Html) {
        let r = Response::default().with_source(base);
        (r, Html::parse_document(body))
    }

    #[test]
    fn test_parse_anchors() {
        let (r, doc) = resp(
            "https://example.com/",
            r##"<a href="/one">1</a><a href="https://other.com/two">2</a><a href="#frag">3</a>"##,
        );
        let reqs = parse_html(&r, &doc, &ParserOptions::default());
        let urls: Vec<&str> = reqs.iter().map(|x| x.url.as_str()).collect();
        assert!(urls.contains(&"https://example.com/one"));
        assert!(urls.contains(&"https://other.com/two"));
        assert!(!urls.iter().any(|u| u.ends_with("#frag")));
        let anchor = reqs.iter().find(|x| x.url.ends_with("/one")).unwrap();
        assert_eq!(anchor.tag, "a");
        assert_eq!(anchor.attribute, "href");
    }

    #[test]
    fn test_parse_scripts_and_images() {
        let (r, doc) = resp(
            "https://example.com/",
            r#"<script src="/app.js"></script><img src="/logo.png" srcset="a.png 1x, b.png 2x"><iframe src="/embed"></iframe>"#,
        );
        let reqs = parse_html(&r, &doc, &ParserOptions::default());
        let urls: Vec<&str> = reqs.iter().map(|x| x.url.as_str()).collect();
        assert!(urls.contains(&"https://example.com/app.js"));
        assert!(urls.contains(&"https://example.com/logo.png"));
        assert!(urls.contains(&"https://example.com/a.png"));
        assert!(urls.contains(&"https://example.com/b.png"));
        assert!(urls.contains(&"https://example.com/embed"));
    }

    #[test]
    fn test_htmx_parsing() {
        let (r, doc) = resp(
            "https://example.com/",
            r#"<button hx-post="/api/save">Save</button><div hx-get="/api/load"></div>"#,
        );
        let reqs = parse_html(&r, &doc, &ParserOptions::default());
        assert!(reqs.iter().any(|x| x.url.ends_with("/api/save") && x.method == "POST"));
        assert!(reqs.iter().any(|x| x.url.ends_with("/api/load") && x.method == "GET"));
    }

    #[test]
    fn test_header_link_parser() {
        let mut r = Response::default().with_source("https://example.com/");
        r.headers
            .insert("Link".into(), "<https://example.com/next>; rel=next".into());
        let doc = Html::parse_document("");
        let reqs = parse_html(&r, &doc, &ParserOptions::default());
        assert!(reqs.iter().any(|x| x.url == "https://example.com/next" && x.tag == "header"));
    }

    #[test]
    fn test_invalid_navigation_filter() {
        let (r, doc) = resp(
            "https://example.com/",
            r#"<a href="javascript:void(0)">x</a><a href="mailto:a@b.c">y</a><a href="data:image/png;base64,xx">z</a>"#,
        );
        let reqs = parse_html(&r, &doc, &ParserOptions::default());
        assert!(reqs.is_empty());
    }

    #[test]
    fn test_extract_forms() {
        let (_, doc) = resp(
            "https://example.com/",
            r#"<form action="/login" method="post"><input name="user"><input name="pass"></form>"#,
        );
        let forms = extract_forms(&doc);
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].method, "POST");
        assert_eq!(forms[0].action, "/login");
        assert_eq!(forms[0].parameters, vec!["user", "pass"]);
    }

    #[test]
    fn test_common_js_library_detection() {
        assert!(is_common_js_library("/static/jquery-3.6.0.min.js"));
        assert!(!is_common_js_library("/static/myapp.js"));
    }
}
