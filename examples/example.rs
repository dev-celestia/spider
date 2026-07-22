use std::env;
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, TimeoutStrategy, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();

    // Parse target URL or default to https://0xbuffer.com/
    let start_url = args
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "https://0xbuffer.com/".to_string());

    let debug_mode = args.iter().any(|arg| arg == "--debug");
    let static_mode = args.iter().any(|arg| arg == "--static");
    let stealth_mode = !args.iter().any(|arg| arg == "--no-stealth");

    let render_mode = if static_mode {
        RenderMode::Static
    } else {
        RenderMode::Dynamic
    };

    println!("=== Browser Crawler Example ===");
    println!("Target URL: {}", start_url);
    println!("Render Mode: {}", if render_mode == RenderMode::Dynamic { "Dynamic (Headless Chrome)" } else { "Static (HTTP)" });
    println!("Stealth Evasion: {}", stealth_mode);
    println!("Debug Inspector: {}", debug_mode);
    println!("Output Directory: ./out");

    // Build the Browser instance using the fluent Builder API
    let browser = Browser::builder()
        .start_url(&start_url)
        .max_depth(1)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .output_dir("out")
        .render_mode(render_mode)
        .stealth(stealth_mode)
        .debug(debug_mode)
        .wait_until(WaitUntil::Delay(Duration::from_secs(3)))
        .render_timeout(Duration::from_secs(12))
        .timeout_strategy(TimeoutStrategy::ExtractPartial)
        .on_page(|page_ir| async move {
            println!("\n--------------------------------------------------");
            println!("[Phase 3 Callback] Processed Page: {}", page_ir.url);
            println!("Title: {}", page_ir.title);
            println!("Extracted Markdown IR Size: {} chars", page_ir.markdown_ir.len());
            println!("--------------------------------------------------");
            Ok(())
        })
        .build()?;

    // Execute the streaming pipeline
    let summary = browser.run().await?;

    println!("\n=== Crawl Summary ===");
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    println!("Visited URLs: {:?}", summary.visited_urls);

    if debug_mode {
        println!("\nCheck ./out/debug_dump.html for the raw live rendered DOM snapshot!");
    }

    Ok(())
}
