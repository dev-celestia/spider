use browser_crawler::{Browser, RenderMode, TimeoutStrategy, WaitUntil};
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), String> {
    let start_url = "https://staging.mycarrier.co.id";

    // Allow customizing output directory via first command-line argument, defaulting to "out"
    let out_dir = env::args().nth(1).unwrap_or_else(|| "out".to_string());
    println!("Target Output Directory: ./{}", out_dir);

    // Build the 4-phase Browser instance with Anti-Bot Stealth Mode enabled
    let browser = Browser::builder()
        .start_url(start_url)
        .max_depth(1)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .output_dir(&out_dir)
        // Enable Headless Chrome rendering
        .render_mode(RenderMode::Dynamic)
        // Enable Anti-Bot Stealth Evasion
        .stealth(true)
        // Playwright-style wait: wait for active network requests to settle
        .wait_until(WaitUntil::NetworkIdle)
        .render_timeout(Duration::from_secs(12))
        .timeout_strategy(TimeoutStrategy::ExtractPartial)
        .on_page(|page_ir| async move {
            println!("[Phase 3] Processed Stealth Page URL: {}", page_ir.url);
            println!("Extracted Title: {}", page_ir.title);
            println!("Extracted Content Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
        .build()?;

    // Execute the 4-phase queue-based streaming pipeline
    let summary = browser.run().await?;
    println!("\n=== Crawl Summary ===");
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    println!("Visited URLs: {:?}", summary.visited_urls);

    Ok(())
}
