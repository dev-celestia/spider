use dashmap::DashSet;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::Arc;
use url::Url;

use crate::types::SitemapNode;

/// Rapid sitemap mapper for Phase 1 link discovery.
///
/// Navigates internal site structure while skipping heavy text rendering to ensure high execution speed.
pub struct SiteMapper {
    client: Client,
    visited: Arc<DashSet<String>>,
    max_depth: usize,
}

impl SiteMapper {
    /// Creates a new `SiteMapper` with the specified maximum crawling depth.
    pub fn new(max_depth: usize) -> Self {
        Self {
            client: Client::builder()
                .user_agent("RustAICrawler/1.0")
                .build()
                .unwrap_or_default(),
            visited: Arc::new(DashSet::new()),
            max_depth,
        }
    }

    /// Asynchronously maps the website starting from `start_url` up to `max_depth`.
    /// Returns the root `SitemapNode` or `None` if the initial request fails.
    pub async fn map_site(&self, start_url: &str) -> Option<SitemapNode> {
        self.crawl_recursive(start_url, 0).await
    }

    async fn crawl_recursive(&self, current_url: &str, depth: usize) -> Option<SitemapNode> {
        if depth > self.max_depth || self.visited.contains(current_url) {
            return None;
        }

        self.visited.insert(current_url.to_string());
        let response = self.client.get(current_url).send().await.ok()?;
        let html = response.text().await.ok()?;

        let document = Html::parse_document(&html);
        let a_selector = Selector::parse("a[href]").ok()?;
        let base_uri = Url::parse(current_url).ok()?;

        let mut children = Vec::new();

        if depth < self.max_depth {
            for element in document.select(&a_selector) {
                if let Some(href) = element.value().attr("href") {
                    if let Ok(joined) = base_uri.join(href) {
                        // Scope crawling exclusively to the same domain host & http/https protocol
                        if joined.host() == base_uri.host()
                            && (joined.scheme() == "http" || joined.scheme() == "https")
                        {
                            let link = joined.to_string();
                            if !self.visited.contains(&link) {
                                if let Some(child) =
                                    Box::pin(self.crawl_recursive(&link, depth + 1)).await
                                {
                                    children.push(child);
                                }
                            }
                        }
                    }
                }
            }
        }

        Some(SitemapNode {
            url: current_url.to_string(),
            depth,
            children,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_site_mapper_creation() {
        let mapper = SiteMapper::new(2);
        assert_eq!(mapper.max_depth, 2);
    }
}
