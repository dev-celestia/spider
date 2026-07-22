use dashmap::DashSet;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::Arc;
use url::Url;

use crate::types::SitemapNode;

/// Builder for constructing [`SiteMapper`] with custom settings.
#[derive(Debug, Clone)]
pub struct SiteMapperBuilder {
    max_depth: usize,
    user_agent: String,
}

impl Default for SiteMapperBuilder {
    fn default() -> Self {
        Self {
            max_depth: 2,
            user_agent: "RustAIBrowser/1.0".to_string(),
        }
    }
}

impl SiteMapperBuilder {
    /// Creates a new `SiteMapperBuilder` with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum crawling depth.
    pub fn max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Sets the User-Agent header for HTTP requests.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// Builds the [`SiteMapper`].
    pub fn build(self) -> SiteMapper {
        let client = Client::builder()
            .user_agent(&self.user_agent)
            .build()
            .unwrap_or_default();

        SiteMapper {
            client,
            visited: Arc::new(DashSet::new()),
            max_depth: self.max_depth,
        }
    }
}

/// Rapid sitemap mapper for Phase 1 link discovery.
///
/// Navigates internal site structure while skipping heavy text rendering to ensure high execution speed.
pub struct SiteMapper {
    client: Client,
    visited: Arc<DashSet<String>>,
    max_depth: usize,
}

impl SiteMapper {
    /// Creates a new `SiteMapperBuilder` instance.
    pub fn builder() -> SiteMapperBuilder {
        SiteMapperBuilder::default()
    }

    /// Creates a new `SiteMapper` with the specified maximum crawling depth.
    pub fn new(max_depth: usize) -> Self {
        Self::builder().max_depth(max_depth).build()
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

        let builder_mapper = SiteMapper::builder().max_depth(5).build();
        assert_eq!(builder_mapper.max_depth, 5);
    }
}
