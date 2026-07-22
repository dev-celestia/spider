use scraper::{Html, Selector};
use crate::types::PageIR;

/// Phase 2: HTML-to-IR Transformer
///
/// Converts verbose raw HTML source into a lightweight, token-efficient `PageIR` payload
/// by extracting document title and translating semantic content blocks (`h1`, `h2`, `h3`, `p`, `li`, `img`, `a`)
/// into formatted Markdown text.
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

    // Isolate core content elements (h1, h2, h3, p, li, img, a)
    let content_selector = Selector::parse("h1, h2, h3, p, li, img, a").unwrap();
    let mut markdown_ir = String::new();

    for element in document.select(&content_selector) {
        let name = element.value().name();

        match name {
            "h1" => {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    markdown_ir.push_str(&format!("\n# {}\n", text));
                }
            }
            "h2" => {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    markdown_ir.push_str(&format!("\n## {}\n", text));
                }
            }
            "h3" => {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    markdown_ir.push_str(&format!("\n### {}\n", text));
                }
            }
            "li" => {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    markdown_ir.push_str(&format!("* {}\n", text));
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
                    let text = element.text().collect::<String>().trim().to_string();
                    if !text.is_empty() && !href.starts_with('#') && !href.starts_with("javascript:") {
                        markdown_ir.push_str(&format!("[{}]({})\n", text, href.trim()));
                    }
                }
            }
            _ => {
                let text = element.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    markdown_ir.push_str(&format!("{}\n\n", text));
                }
            }
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
    fn test_transform_html_to_ir_untitled_fallback() {
        let html = "<div><p>No title tag here</p></div>";
        let ir = transform_html_to_ir("https://example.com/no-title", html);
        assert_eq!(ir.title, "Untitled Page");
        assert!(ir.markdown_ir.contains("No title tag here"));
    }
}
