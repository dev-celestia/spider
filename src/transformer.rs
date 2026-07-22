use scraper::{Html, Selector};
use crate::types::PageIR;

/// Phase 2: HTML-to-IR Transformer
///
/// Converts verbose raw HTML source into a lightweight, token-efficient `PageIR` payload
/// by extracting document title and translating semantic content blocks (`h1`, `h2`, `h3`, `p`, `li`)
/// into formatted Markdown text.
///
/// # Arguments
/// * `url` - Source URL of the HTML document.
/// * `html` - Raw HTML string to transform.
///
/// # Examples
/// ```
/// use browser_crawler::transform_html_to_ir;
///
/// let html = "<html><head><title>My Page</title></head><body><h1>Hello</h1><p>World</p></body></html>";
/// let ir = transform_html_to_ir("https://example.com", html);
/// assert_eq!(ir.title, "My Page");
/// assert!(ir.markdown_ir.contains("# Hello"));
/// ```
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

    // Isolate core content elements (h1, h2, h3, p, li)
    let content_selector = Selector::parse("h1, h2, h3, p, li").unwrap();
    let mut markdown_ir = String::new();

    for element in document.select(&content_selector) {
        let text = element.text().collect::<String>().trim().to_string();
        if text.is_empty() {
            continue;
        }

        match element.value().name() {
            "h1" => markdown_ir.push_str(&format!("\n# {}\n", text)),
            "h2" => markdown_ir.push_str(&format!("\n## {}\n", text)),
            "h3" => markdown_ir.push_str(&format!("\n### {}\n", text)),
            "li" => markdown_ir.push_str(&format!("* {}\n", text)),
            _ => markdown_ir.push_str(&format!("{}\n\n", text)),
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
        assert!(ir.markdown_ir.contains("* Second item"));
    }

    #[test]
    fn test_transform_html_to_ir_untitled_fallback() {
        let html = "<div><p>No title tag here</p></div>";
        let ir = transform_html_to_ir("https://example.com/no-title", html);
        assert_eq!(ir.title, "Untitled Page");
        assert!(ir.markdown_ir.contains("No title tag here"));
    }
}
