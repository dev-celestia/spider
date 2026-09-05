//! Port of the reference crawler `pkg/engine/parser/files` — known-files crawling
//! (robots.txt / sitemap.xml) with parsing helpers.

use crate::types::result::{Request, Response};

/// Which known files to visit (reference crawler `-kf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownFilesMode {
    RobotsTxt,
    SitemapXml,
    All,
}

/// Return the known-file URLs to fetch for a target root (reference crawler `Visit`).
pub fn known_file_urls(base_url: &str, mode: KnownFilesMode) -> Vec<String> {
    let base = base_url.trim_end_matches('/');
    let mut urls = Vec::new();
    match mode {
        KnownFilesMode::RobotsTxt => urls.push(format!("{base}/robots.txt")),
        KnownFilesMode::SitemapXml => urls.push(format!("{base}/sitemap.xml")),
        KnownFilesMode::All => {
            urls.push(format!("{base}/robots.txt"));
            urls.push(format!("{base}/sitemap.xml"));
        }
    }
    urls
}

/// Parse robots.txt content returning navigation requests for allow/disallow
/// paths and sitemap directives (reference crawler `robotstxt.go parseReader`).
///
/// Depth is set to 2 matching the reference crawler's known-files depth.
pub fn parse_robots_txt(base_url: &str, body: &str) -> Vec<Request> {
    let resp = Response::default().with_source(base_url);
    let mut requests = Vec::new();
    for line in body.lines() {
        let Some((directive, value)) = line.split_once(": ") else {
            continue;
        };
        let d = directive.to_lowercase();
        if d.starts_with("allow") || d == "disallow" {
            let req = Request::from_response(value.trim(), base_url, "robotstxt", &d, &resp);
            if !req.url.is_empty() {
                requests.push(req);
            }
        } else if d == "sitemap" {
            let req = Request::from_response(value.trim(), base_url, "robotstxt", "sitemap", &resp);
            if !req.url.is_empty() {
                requests.push(req);
            }
        }
    }
    requests
}

/// Parse sitemap.xml content returning navigation requests for every `<loc>`
/// (reference crawler `sitemapxml.go parseReader`).
pub fn parse_sitemap_xml(base_url: &str, body: &str) -> Vec<Request> {
    let resp = Response::default().with_source(base_url);
    let mut requests = Vec::new();
    // Lightweight loc extraction without a full XML dependency.
    for rest in body.split("<loc>") {
        let Some(end) = rest.find("</loc>") else { continue };
        let loc = rest[..end].trim();
        if loc.is_empty() {
            continue;
        }
        let req = Request::from_response(loc, base_url, "sitemapxml", "loc", &resp);
        if !req.url.is_empty() {
            requests.push(req);
        }
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_file_urls() {
        assert_eq!(
            known_file_urls("https://x.com/", KnownFilesMode::RobotsTxt),
            vec!["https://x.com/robots.txt".to_string()]
        );
        assert_eq!(known_file_urls("https://x.com", KnownFilesMode::All).len(), 2);
    }

    #[test]
    fn test_parse_robots() {
        let robots = "User-agent: *\nDisallow: /admin\nAllow: /public/\nSitemap: https://x.com/sitemap.xml";
        let reqs = parse_robots_txt("https://x.com/robots.txt", robots);
        let urls: Vec<&str> = reqs.iter().map(|r| r.url.as_str()).collect();
        assert!(urls.contains(&"https://x.com/admin"));
        assert!(urls.contains(&"https://x.com/public/"));
        assert!(urls.contains(&"https://x.com/sitemap.xml"));
    }

    #[test]
    fn test_parse_sitemap() {
        let xml = "<?xml version=\"1.0\"?><urlset><url><loc>https://x.com/a</loc></url><url><loc>https://x.com/b</loc></url></urlset>";
        let reqs = parse_sitemap_xml("https://x.com/sitemap.xml", xml);
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].url, "https://x.com/a");
    }
}
