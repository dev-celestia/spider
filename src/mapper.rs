use dashmap::DashSet;
use reqwest::Client;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::renderer::PageFetcher;
use crate::transformer::extract_links;
use crate::types::{RenderOptions, SitemapNode};

/// Builder for constructing [`SiteMapper`] with custom settings.
#[derive(Debug, Clone)]
pub struct SiteMapperBuilder {
    max_depth: usize,
    max_pages: usize,
    user_agent: String,
    render_options: RenderOptions,
}

impl Default for SiteMapperBuilder {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_pages: 0,
            user_agent: "RustAIBrowser/1.0".to_string(),
            render_options: RenderOptions::default(),
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

    /// Sets the maximum number of pages to fetch (`0` = unlimited).
    ///
    /// DFS expansion stops once the cap is reached, protecting against
    /// unbounded recursion on large sites.
    pub fn max_pages(mut self, max_pages: usize) -> Self {
        self.max_pages = max_pages;
        self
    }

    /// Sets the User-Agent header for HTTP requests.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// Sets page rendering configuration.
    pub fn render_options(mut self, options: RenderOptions) -> Self {
        self.render_options = options;
        self
    }

    /// Builds the [`SiteMapper`].
    pub fn build(self) -> SiteMapper {
        let client = Client::builder()
            .user_agent(&self.user_agent)
            .build()
            .unwrap_or_default();

        let fetcher = PageFetcher::new(client, self.render_options);

        SiteMapper {
            fetcher,
            visited: Arc::new(DashSet::new()),
            pages_fetched: Arc::new(AtomicUsize::new(0)),
            max_depth: self.max_depth,
            max_pages: self.max_pages,
        }
    }
}

/// Rapid sitemap mapper for Phase 1 link discovery.
///
/// Traverses the site **depth-first** (recursive): each page's links are
/// followed to their maximum depth before backtracking, building a
/// [`SitemapNode`] tree on the way. A link reachable from multiple parents
/// appears only once — under the first DFS path that reaches it. For a flat,
/// concurrent scan instead, use the crawl engine's `-s breadth-first`
/// strategy.
pub struct SiteMapper {
    fetcher: PageFetcher,
    visited: Arc<DashSet<String>>,
    pages_fetched: Arc<AtomicUsize>,
    max_depth: usize,
    max_pages: usize,
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

    /// Number of pages fetched so far (respects `max_pages`).
    pub fn pages_fetched(&self) -> usize {
        self.pages_fetched.load(Ordering::Relaxed)
    }

    async fn crawl_recursive(&self, current_url: &str, depth: usize) -> Option<SitemapNode> {
        if depth > self.max_depth || self.visited.contains(current_url) {
            return None;
        }

        self.visited.insert(current_url.to_string());
        if self.max_pages > 0 {
            if self.pages_fetched.load(Ordering::Relaxed) >= self.max_pages {
                return None;
            }
            self.pages_fetched.fetch_add(1, Ordering::Relaxed);
        }

        let html = self.fetcher.fetch_html(current_url).await.ok()?;
        let links = extract_links(current_url, &html).unwrap_or_default();

        let mut children = Vec::new();

        if depth < self.max_depth {
            for link in links {
                if !self.visited.contains(&link) {
                    if let Some(child) = Box::pin(self.crawl_recursive(&link, depth + 1)).await {
                        children.push(child);
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

    #[tokio::test]
    async fn test_site_mapper_max_pages() {
        let mapper = SiteMapper::builder().max_depth(5).max_pages(10).build();
        assert_eq!(mapper.max_pages, 10);

        let unlimited = SiteMapper::builder().build();
        assert_eq!(unlimited.max_pages, 0);
    }
}
