//! Port of the reference crawler `pkg/engine/hybrid` — hybrid crawling: pages render through
//! headless Chrome (live DOM + XHR hooks), while static sub-resources (JS/CSS)
//! are fetched over plain HTTP like the standard engine.

use std::sync::Arc;

use async_trait::async_trait;

use crate::control::CrawlControl;
use crate::engine::common::{Crawler, PageFetch};
use crate::engine::headless::HeadlessFetcher;
use crate::engine::standard::StandardFetcher;
use crate::output::StandardWriter;
use crate::types::options::Options;
use crate::types::result::{Request, Response};

/// Hybrid fetcher: headless for documents, HTTP for static resources.
pub struct HybridFetcher {
    headless: HeadlessFetcher,
    standard: StandardFetcher,
}

impl HybridFetcher {
    pub fn new(options: Arc<Options>, control: Arc<CrawlControl>) -> std::result::Result<Self, String> {
        Ok(HybridFetcher {
            headless: HeadlessFetcher::new(Arc::clone(&options), control)?,
            standard: StandardFetcher::from_options(&options)?,
        })
    }
}

/// True when the URL looks like a static resource fetched better over HTTP.
fn is_static_resource(url: &str) -> bool {
    let path = url::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string());
    let lower = path.to_lowercase();
    lower.ends_with(".js")
        || lower.ends_with(".css")
        || lower.ends_with(".json")
        || lower.ends_with(".xml")
        || lower.ends_with(".txt")
}

#[async_trait]
impl PageFetch for HybridFetcher {
    async fn fetch(&self, request: &Request) -> std::result::Result<Response, String> {
        if is_static_resource(&request.url) {
            self.standard.fetch(request).await
        } else {
            self.headless.fetch(request).await
        }
    }
}

/// Build and run a hybrid-engine crawler for one seed (reference crawler `-hh`).
pub async fn crawl_hybrid(
    options: Arc<Options>,
    writer: Arc<StandardWriter>,
    control: Arc<CrawlControl>,
    seed: &str,
) -> std::result::Result<Arc<Crawler>, String> {
    let fetcher = Arc::new(HybridFetcher::new(Arc::clone(&options), Arc::clone(&control))?);
    let crawler = Arc::new(Crawler::new(options, fetcher, writer, control)?);
    crawler.crawl(seed).await?;
    Ok(crawler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_resource_detection() {
        assert!(is_static_resource("https://x.com/app.js"));
        assert!(is_static_resource("https://x.com/style.css"));
        assert!(!is_static_resource("https://x.com/page.php"));
        assert!(!is_static_resource("https://x.com/"));
    }

    #[test]
    fn test_hybrid_fetcher_new() {
        let o = Arc::new(Options::with_defaults());
        assert!(HybridFetcher::new(o, Arc::new(CrawlControl::default())).is_ok());
    }
}
