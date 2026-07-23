use scraper::{Html, Selector, ElementRef};
use crate::types::PageIR;

/// Checks if an element is contained within an ignored tag (script, style, head, noscript, template).
fn is_inside_ignored_tag(element: &ElementRef) -> bool {
    let mut current = element.parent();
    while let Some(parent_node) = current {
        if let Some(parent_elem) = ElementRef::wrap(parent_node) {
            let name = parent_elem.value().name();
            if name == "script" || name == "style" || name == "head" || name == "noscript" || name == "template" {
                return true;
            }
        }
        current = parent_node.parent();
    }
    false
}

/// Checks if an element contains block-level descendant elements.
fn has_block_descendants(element: &ElementRef) -> bool {
    let block_selector = Selector::parse("h1, h2, h3, p, li, div, section, article, table, tr, form, blockquote, pre").unwrap();
    element.select(&block_selector).next().is_some()
}

/// Recursively collects text nodes while ignoring script, style, and noscript element subtrees.
fn collect_text_nodes(element: &ElementRef, buf: &mut String) {
    for child in element.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            let name = child_elem.value().name();
            if name == "script" || name == "style" || name == "noscript" {
                continue;
            }
            collect_text_nodes(&child_elem, buf);
        } else if let Some(text) = child.value().as_text() {
            buf.push_str(text);
            buf.push(' ');
        }
    }
}

/// Extracts text from an element while ignoring child script/style/noscript text nodes.
fn extract_clean_text(element: &ElementRef) -> String {
    let mut text_buf = String::new();
    collect_text_nodes(element, &mut text_buf);
    text_buf.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Phase 2: HTML-to-IR Transformer
///
/// Converts verbose raw HTML source into a lightweight, token-efficient `PageIR` payload
/// by extracting document title and translating semantic content blocks (`h1`, `h2`, `h3`, `p`, `li`, `img`, `a`, `div`, `span`, `code`, `pre`)
/// into formatted Markdown text while stripping script, style, and layout wrapper noise.
pub fn transform_html_to_ir(url: &str, html: &str) -> PageIR {
    let document = Html::parse_document(html);

    // Extract title
    let title_selector = Selector::parse("title").unwrap();
    let title = document
        .select(&title_selector)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled Page".to_string());

    // Isolate core content elements (h1, h2, h3, p, li, img, a, div, span, code, pre, td, th, blockquote)
    let content_selector = Selector::parse("h1, h2, h3, p, li, img, a, div, span, code, pre, td, th, blockquote").unwrap();
    let mut markdown_ir = String::new();
    let mut last_added_text = String::new();

    for element in document.select(&content_selector) {
        if is_inside_ignored_tag(&element) {
            continue;
        }

        let name = element.value().name();

        match name {
            "h1" => {
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("\n# {}\n", text));
                    last_added_text = text;
                }
            }
            "h2" => {
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("\n## {}\n", text));
                    last_added_text = text;
                }
            }
            "h3" => {
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("\n### {}\n", text));
                    last_added_text = text;
                }
            }
            "li" => {
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("* {}\n", text));
                    last_added_text = text;
                }
            }
            "img" => {
                if let Some(src) = element.value().attr("src") {
                    let alt = element.value().attr("alt").unwrap_or("Thumbnail");
                    if !src.is_empty() {
                        markdown_ir.push_str(&format!("\n![{}]({})\n", alt.trim(), src.trim()));
                    }
                }
            }
            "a" => {
                if let Some(href) = element.value().attr("href") {
                    let text = extract_clean_text(&element);
                    if !text.is_empty() && !href.starts_with('#') && !href.starts_with("javascript:") {
                        markdown_ir.push_str(&format!("[{}]({})\n", text, href.trim()));
                    }
                }
            }
            "pre" => {
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("\n```\n{}\n```\n", text));
                    last_added_text = text;
                }
            }
            "code" => {
                if let Some(parent) = element.parent() {
                    if let Some(elem) = ElementRef::wrap(parent) {
                        if elem.value().name() == "pre" {
                            continue;
                        }
                    }
                }
                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!(" `{}` ", text));
                    last_added_text = text;
                }
            }
            "div" | "span" | "p" | "td" | "th" | "blockquote" => {
                if name == "div" && has_block_descendants(&element) {
                    continue;
                }
                if name == "span" {
                    if let Some(parent) = element.parent() {
                        if let Some(p_elem) = ElementRef::wrap(parent) {
                            let p_name = p_elem.value().name();
                            if matches!(p_name, "div" | "p" | "td" | "th" | "li" | "h1" | "h2" | "h3" | "blockquote") {
                                continue;
                            }
                        }
                    }
                }

                let text = extract_clean_text(&element);
                if !text.is_empty() && text != last_added_text {
                    markdown_ir.push_str(&format!("{}\n\n", text));
                    last_added_text = text;
                }
            }
            _ => {}
        }
    }

    PageIR {
        url: url.to_string(),
        title,
        markdown_ir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_html_to_ir_basic() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Test Page</title></head>
            <body>
                <h1>Main Heading</h1>
                <p>Hello world paragraph.</p>
                <h2>Sub Heading</h2>
                <ul>
                    <li>First item</li>
                    <li>Second item</li>
                </ul>
                <img src="https://example.com/thumb.jpg" alt="Article Thumbnail" />
                <a href="https://example.com/article/1">Read Article 1</a>
            </body>
            </html>
        "#;

        let ir = transform_html_to_ir("https://example.com", html);
        assert_eq!(ir.url, "https://example.com");
        assert_eq!(ir.title, "Test Page");
        assert!(ir.markdown_ir.contains("# Main Heading"));
        assert!(ir.markdown_ir.contains("Hello world paragraph."));
        assert!(ir.markdown_ir.contains("## Sub Heading"));
        assert!(ir.markdown_ir.contains("* First item"));
        assert!(ir.markdown_ir.contains("![Article Thumbnail](https://example.com/thumb.jpg)"));
        assert!(ir.markdown_ir.contains("[Read Article 1](https://example.com/article/1)"));
    }

    #[test]
    fn test_transform_html_to_ir_div_span_code() {
        let html = r#"
            <div>Direct text inside un-wrapped div</div>
            <span>Standalone span text</span>
            <pre>API_SECRET=super_secret_12345</pre>
            <script>var ignored = 'should_not_appear';</script>
            <style>.ignored { color: red; }</style>
        "#;

        let ir = transform_html_to_ir("https://example.com", html);
        assert!(ir.markdown_ir.contains("Direct text inside un-wrapped div"));
        assert!(ir.markdown_ir.contains("Standalone span text"));
        assert!(ir.markdown_ir.contains("API_SECRET=super_secret_12345"));
        assert!(!ir.markdown_ir.contains("should_not_appear"));
        assert!(!ir.markdown_ir.contains(".ignored"));
    }

    #[test]
    fn test_transform_html_to_ir_untitled_fallback() {
        let html = "<div><p>No title tag here</p></div>";
        let ir = transform_html_to_ir("https://example.com/no-title", html);
        assert_eq!(ir.title, "Untitled Page");
        assert!(ir.markdown_ir.contains("No title tag here"));
    }
}


