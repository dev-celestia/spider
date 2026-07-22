use browser_crawler::Browser;
use std::env;

#[tokio::main]
async fn main() -> Result<(), String> {
    let start_url = "https://0xbuffer.com";

    // Allow customizing output directory via first command-line argument, defaulting to "out"
    let out_dir = env::args().nth(1).unwrap_or_else(|| "out".to_string());
    println!("Target Output Directory: ./{}", out_dir);

    // Build the 4-phase Browser instance using the fluent Builder pattern
    let browser = Browser::builder()
        .start_url(start_url)
        .max_depth(2)
        .user_agent("RustAIBrowser/1.0")
        .output_dir(&out_dir)
        .on_page(|page_ir| async move {
            println!("[Phase 3] Processing URL: {}", page_ir.url);
            println!("Extracted Text Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
        .build()?;

    // Execute the full 4-phase pipeline
    browser.run().await?;

    Ok(())
}
