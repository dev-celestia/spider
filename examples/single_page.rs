use browser_crawler::{crawl_single_page, extract_links, Browser, RenderMode, RenderOptions};

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("=== Single Page Crawl & Link Extraction Example ===");

    // 1. One-off single page crawl using `crawl_single_page` function
    let target_url = "https://example.com";
    println!("\n1. Crawling single page: {}", target_url);

    let options = RenderOptions {
        render_mode: RenderMode::Static,
        ..Default::default()
    };

    let page_ir = crawl_single_page(target_url, &options).await?;
    println!("   URL: {}", page_ir.url);
    println!("   Title: {}", page_ir.title);
    println!("   Markdown Length: {} chars", page_ir.markdown_ir.len());

    // 2. Extract same-domain links using `extract_links` function
    let sample_html = r#"
        <html>
            <body>
                <a href="/about">About Us</a>
                <a href="/docs">Documentation</a>
                <a href="https://example.com/blog">Blog</a>
                <a href="https://external.org">External Link</a>
            </body>
        </html>
    "#;
    println!("\n2. Extracting same-domain links from HTML...");
    let links = extract_links("https://example.com", sample_html)?;
    for (idx, link) in links.iter().enumerate() {
        println!("   [{}] {}", idx + 1, link);
    }

    // 3. Fetching a page using `Browser::fetch_page` method
    println!("\n3. Fetching single page using Browser instance...");
    let browser = Browser::builder()
        .start_url(target_url)
        .render_mode(RenderMode::Static)
        .build()?;

    let fetched = browser.fetch_page(target_url).await?;
    println!("   Fetched Title: {}", fetched.title);

    Ok(())
}
