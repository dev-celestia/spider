//! Port of the reference crawler `pkg/engine/standard` — the plain-HTTP crawl engine built on
//! `reqwest` (retries live in the shared engine loop).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::engine::common::{Crawler, PageFetch};
use crate::output::StandardWriter;
use crate::types::options::Options;
use crate::types::result::{Headers, Request, Response};

/// HTTP fetcher using `reqwest` (celestia standard engine).
pub struct StandardFetcher {
    client: reqwest::Client,
    body_read_size: usize,
    custom_headers: Headers,
}

impl StandardFetcher {
    /// Build a fetcher from options (timeout, proxy, headers, redirects).
    pub fn from_options(options: &Options) -> std::result::Result<StandardFetcher, String> {
        let mut builder = reqwest::Client::builder()
            .user_agent(crate::utils::regex::web_user_agent())
            .timeout(Duration::from_secs(options.timeout.max(1)));

        if options.disable_redirects {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        } else {
            builder = builder.redirect(reqwest::redirect::Policy::limited(10));
        }

        if !options.proxy.is_empty() {
            let proxy = reqwest::Proxy::all(&options.proxy)
                .map_err(|e| format!("invalid proxy url: {e}"))?;
            builder = builder.proxy(proxy);
        }

        let client = builder
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;
        Ok(StandardFetcher {
            client,
            body_read_size: options.body_read_size,
            custom_headers: options.custom_headers.clone(),
        })
    }

    /// Merge per-request headers over the global custom headers
    /// (reference crawler `ParseCustomHeaders` applied at request time).
    fn merge_headers(&self, request: &Request) -> Headers {
        let mut headers = self.custom_headers.clone();
        for (k, v) in &request.headers {
            headers.insert(k.clone(), v.clone());
        }
        headers
    }
}

#[async_trait]
impl PageFetch for StandardFetcher {
    async fn fetch(&self, request: &Request) -> std::result::Result<Response, String> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .unwrap_or(reqwest::Method::GET);

        let mut req = self.client.request(method, &request.url);
        for (k, v) in &self.merge_headers(request) {
            req = req.header(k, v);
        }
        if !request.body.is_empty() {
            req = req.body(request.body.clone());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status_code = resp.status().as_u16() as i32;
        let mut headers: Headers = Headers::new();
        for (k, v) in resp.headers() {
            headers.insert(k.to_string(), v.to_str().unwrap_or("").to_string());
        }
        let content_length = resp.content_length().unwrap_or(0) as i64;
        let body = resp
            .text()
            .await
            .map_err(|e| format!("failed to read response body: {e}"))?;
        let body = body[..body.len().min(self.body_read_size)].to_string();

        Ok(Response {
            depth: request.depth,
            status_code,
            headers,
            content_length,
            body,
            root_hostname: request.root_hostname.clone(),
            source: request.url.clone(),
            ..Default::default()
        })
    }
}

/// Build and run a standard-engine crawler for one seed
/// (reference crawler `standard.Crawler.Crawl`).
pub async fn crawl_standard(
    options: Arc<Options>,
    writer: Arc<StandardWriter>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    seed: &str,
) -> std::result::Result<Arc<Crawler>, String> {
    let fetcher = Arc::new(StandardFetcher::from_options(&options)?);
    let crawler = Arc::new(Crawler::new(options, fetcher, writer, cancel)?);
    crawler.crawl(seed).await?;
    Ok(crawler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_from_options() {
        let o = Options::with_defaults();
        let f = StandardFetcher::from_options(&o);
        assert!(f.is_ok());
    }

    #[test]
    fn test_fetcher_with_proxy_and_no_redirects() {
        let mut o = Options::with_defaults();
        o.proxy = "http://127.0.0.1:9999".into();
        o.disable_redirects = true;
        assert!(StandardFetcher::from_options(&o).is_ok());
    }

    #[test]
    fn test_invalid_proxy_rejected() {
        let mut o = Options::with_defaults();
        o.proxy = "not a url".into();
        assert!(StandardFetcher::from_options(&o).is_err());
    }

    #[test]
    fn test_merge_headers_request_wins() {
        let mut o = Options::with_defaults();
        o.custom_headers.insert("X-Global".into(), "g".into());
        let f = StandardFetcher::from_options(&o).unwrap();
        let mut req = Request::default();
        req.headers.insert("X-Global".into(), "local".into());
        let merged = f.merge_headers(&req);
        assert_eq!(merged.get("X-Global").unwrap(), "local");
    }
}
