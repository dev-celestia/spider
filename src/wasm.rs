//! WebAssembly bindings exposing the pure, I/O-free parts of the library to
//! JavaScript (built with `wasm-pack build --target web`). The network,
//! headless-browser, and filesystem engines are unavailable on wasm32; the
//! browser supplies fetching (e.g. via `fetch`) and hands the HTML to these
//! entry points.

use wasm_bindgen::prelude::*;

use crate::engine::parser;
use crate::transformer;

/// Library version, surfaced in the playground UI status badge.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Transform raw HTML into the AI intermediate representation (`PageIR`).
/// Returns a plain JS object: `{ url, title, markdown_ir }`.
#[wasm_bindgen]
pub fn transform_to_ir(url: String, html: String) -> Result<JsValue, JsValue> {
    let ir = transformer::transform_html_to_ir(&url, &html);
    serde_wasm_bindgen::to_value(&ir).map_err(Into::into)
}

/// Extract navigation links from HTML, resolved against `base_url`.
/// Same-host only, matching the crawler's queue-discovery semantics.
#[wasm_bindgen]
pub fn extract_links(base_url: String, html: String) -> Result<Vec<String>, JsValue> {
    transformer::extract_links(&base_url, &html).map_err(Into::into)
}

/// Extract every HTTP(S) link (internal and external) resolved against `base_url`.
#[wasm_bindgen]
pub fn extract_all_links(base_url: String, html: String) -> Result<Vec<String>, JsValue> {
    let parsed_base =
        url::Url::parse(&base_url).map_err(|e| JsValue::from_str(&format!("invalid base URL: {e}")))?;
    let document = scraper::Html::parse_document(&html);
    let selector =
        scraper::Selector::parse("a[href]").map_err(|_| JsValue::from_str("selector error"))?;

    let mut links = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            let href = href.trim();
            if href.starts_with('#')
                || href.starts_with("javascript:")
                || href.starts_with("mailto:")
            {
                continue;
            }
            if let Ok(mut joined) = parsed_base.join(href) {
                if joined.scheme() == "http" || joined.scheme() == "https" {
                    joined.set_fragment(None);
                    let link = joined.to_string();
                    if seen.insert(link.clone()) {
                        links.push(link);
                    }
                }
            }
        }
    }
    Ok(links)
}

/// Extract HTML forms (action, method, inputs) as an array of JS objects.
#[wasm_bindgen]
pub fn extract_forms(html: String) -> Result<JsValue, JsValue> {
    let document = scraper::Html::parse_document(&html);
    let forms = parser::extract_forms(&document);
    serde_wasm_bindgen::to_value(&forms).map_err(Into::into)
}

/// Extract endpoint candidates from JavaScript source (jsluice-style heuristics).
#[wasm_bindgen]
pub fn extract_js_endpoints(content: String) -> Vec<String> {
    parser::jsluice_endpoints(&content)
}
