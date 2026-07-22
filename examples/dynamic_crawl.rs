use std::env;
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, TimeoutStrategy, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), String> {
    let start_url = "https://staging.mycarrier.co.id";

    // Allow customizing output directory via first command-line argument, defaulting to "out"
    let out_dir = env::args().nth(1).unwrap_or_else(|| "out".to_string());
    println!("Target Output Directory: ./{}", out_dir);

    // Build the 4-phase Browser instance with Headless Chrome dynamic JS/CSS rendering
    let browser = Browser::builder()
        .start_url(start_url)
        .max_depth(1)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .output_dir(&out_dir)
        // Enable Headless Chrome rendering for client-side JS/CSS dynamic content
        .render_mode(RenderMode::Dynamic)
        // Set fixed 3-second render settlement delay to allow heavy JS/CSS dynamic hydration
        .wait_until(WaitUntil::Delay(Duration::from_secs(3)))
        .render_timeout(Duration::from_secs(10))
        .timeout_strategy(TimeoutStrategy::ExtractPartial)
        .on_page(|page_ir| async move {
            println!("[Phase 3] Processed Dynamic Page URL: {}", page_ir.url);
            println!("Extracted Title: {}", page_ir.title);
            println!("Extracted Text Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
        .build()?;

    // Execute the full 4-phase queue-based streaming pipeline
    let summary = browser.run().await?;
    println!("\n=== Crawl Summary ===");
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    println!("Visited URLs: {:?}", summary.visited_urls);

    Ok(())
}
