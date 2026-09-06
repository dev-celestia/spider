//! Integration tests for the DFS sitemap-tree crawler (`SiteMapper`).
//! Uses the shared local HTTP test server to verify tree shape, depth
//! increments, cross-page dedup, and the `max_depth` / `max_pages` caps.

mod common;

use common::{spawn, standard_site};
use browser_crawler::types::SitemapNode;

/// Collect all URLs in the tree (pre-order DFS).
fn collect_urls(node: &SitemapNode, out: &mut Vec<String>) {
    out.push(node.url.clone());
    for child in &node.children {
        collect_urls(child, out);
    }
}

fn find_child<'a>(node: &'a SitemapNode, suffix: &str) -> Option<&'a SitemapNode> {
    node.children.iter().find(|c| c.url.ends_with(suffix))
}

#[tokio::test]
async fn dfs_tree_shape_and_depths() {
    let server = spawn(standard_site(None)).await;
    let mapper = browser_crawler::mapper::SiteMapper::builder().max_depth(2).build();

    let root = mapper
        .map_site(&format!("{}/", server.base_url))
        .await
        .expect("root node");

    assert_eq!(root.depth, 0);
    assert!(root.url.ends_with('/'));

    // Root's direct children sit at depth 1.
    let about = find_child(&root, "/about").expect("/about child of root");
    assert_eq!(about.depth, 1);

    // DFS dives first into /about, so /contact is /about's child at depth 2.
    let contact = find_child(about, "/contact").expect("/contact child of /about");
    assert_eq!(contact.depth, 2);

    // Other root children are leaves at depth 1 (no outbound links).
    let users1 = find_child(&root, "/users/1").expect("/users/1 child of root");
    assert_eq!(users1.depth, 1);
    assert!(users1.children.is_empty());

    // Query-param variants are distinct nodes.
    assert!(find_child(&root, "/page?a=1").is_some());
    assert!(find_child(&root, "/page?a=2").is_some());

    server.shutdown();
}

#[tokio::test]
async fn cross_page_dedup_appears_once() {
    let server = spawn(standard_site(None)).await;
    let mapper = browser_crawler::mapper::SiteMapper::builder().max_depth(2).build();

    let root = mapper
        .map_site(&format!("{}/", server.base_url))
        .await
        .expect("root node");

    let mut urls = Vec::new();
    collect_urls(&root, &mut urls);

    // /contact is linked from both / and /about; DFS visits it via /about
    // first, so it must appear exactly once in the tree.
    assert_eq!(
        urls.iter().filter(|u| u.ends_with("/contact")).count(),
        1,
        "/contact should appear exactly once"
    );

    server.shutdown();
}

#[tokio::test]
async fn max_depth_cutoff() {
    let server = spawn(standard_site(None)).await;
    let mapper = browser_crawler::mapper::SiteMapper::builder().max_depth(1).build();

    let root = mapper
        .map_site(&format!("{}/", server.base_url))
        .await
        .expect("root node");

    // Depth 1 => children exist but no grandchildren.
    assert!(!root.children.is_empty());
    for child in &root.children {
        assert_eq!(child.depth, 1);
        assert!(child.children.is_empty(), "no nodes beyond max_depth");
    }

    server.shutdown();
}

#[tokio::test]
async fn max_pages_cap_truncates() {
    let server = spawn(standard_site(None)).await;
    let mapper = browser_crawler::mapper::SiteMapper::builder()
        .max_depth(2)
        .max_pages(3)
        .build();

    let root = mapper
        .map_site(&format!("{}/", server.base_url))
        .await
        .expect("root node");

    assert_eq!(mapper.pages_fetched(), 3, "fetching stops at the cap");

    let mut urls = Vec::new();
    collect_urls(&root, &mut urls);
    assert!(urls.len() <= 3, "tree is truncated to the capped pages");

    server.shutdown();
}

#[tokio::test]
async fn unreachable_root_returns_none() {
    // Port 1 on loopback refuses connections => the root fetch fails.
    let mapper = browser_crawler::mapper::SiteMapper::new(1);

    let result = mapper.map_site("http://127.0.0.1:1/").await;
    assert!(result.is_none(), "unreachable root yields no node");
}
