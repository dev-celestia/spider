//! Shared end-to-end test fixtures: HTTP test servers (async + blocking)
//! with static routes, request recording, and per-path hit counts.
#![allow(dead_code)]


use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A route response: status, content type, body.
#[derive(Clone, Debug)]
pub struct Route {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Route {
    pub fn html(body: impl Into<String>) -> Route {
        Route { status: 200, content_type: "text/html; charset=utf-8", body: body.into() }
    }

    pub fn text(body: impl Into<String>) -> Route {
        Route { status: 200, content_type: "text/plain", body: body.into() }
    }
}

/// Handle to a running test server.
pub struct TestServer {
    pub base_url: String,
    /// Every request path seen by the server, in order (with query strings).
    pub requests: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicUsize>,
    pub addr: std::net::SocketAddr,
}

impl TestServer {
    /// Number of times a path (prefix match on the raw path, pre-query) was hit.
    pub fn hits(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.split('?').next() == Some(path))
            .count()
    }

    pub fn all_requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    /// Signal the server loop to stop; the task ends on its own.
    pub fn shutdown(&self) {
        self.shutdown.store(1, Ordering::SeqCst);
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Spawn a test HTTP server serving the given routes on an ephemeral port.
/// Unknown paths return 404. Request paths are recorded.
pub async fn spawn(routes: HashMap<String, Route>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{}", addr);

    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(AtomicUsize::new(0));
    let routes = Arc::new(routes);

    let requests_task = Arc::clone(&requests);
    let shutdown_task = Arc::clone(&shutdown);

    tokio::spawn(async move {
        loop {
            if shutdown_task.load(Ordering::SeqCst) == 1 {
                return;
            }
            let (stream, _) = tokio::select! {
                l = listener.accept() => match l {
                    Ok(x) => x,
                    Err(_) => return,
                },
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => continue,
            };
            let routes = Arc::clone(&routes);
            let requests = Arc::clone(&requests_task);
            tokio::spawn(async move {
                let _ = handle_conn(stream, routes, requests).await;
            });
        }
    });

    TestServer { base_url, requests, shutdown, addr }
}

async fn handle_conn(
    mut stream: TcpStream,
    routes: Arc<HashMap<String, Route>>,
    requests: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
    let Some(request_line) = raw.lines().next() else { return Ok(()) };
    let mut parts = request_line.split_whitespace();
    let _method = parts.next().unwrap_or("GET");
    let path_with_query = parts.next().unwrap_or("/").to_string();
    let path = path_with_query.split('?').next().unwrap_or("/").to_string();

    requests.lock().unwrap().push(path_with_query);

    let route = routes.get(&path);
    let (status, content_type, body) = match route {
        Some(r) => (r.status, r.content_type, r.body.clone()),
        None => (404, "text/plain", "not found".to_string()),
    };
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// The standard multi-page test site used across E2E tests.
///
/// `external_link` (optional) is inserted on the index page as an absolute URL
/// to another server — used by scope tests.
///
/// Structure:
/// ```text
/// /                 -> links /about /contact /form /page?a=1 /page?a=2 /users/1 /users/2
///                      plus /logo.png (image) and /app.js (script)
/// /about            -> link /contact
/// /contact          -> (leaf)
/// /logo.png         -> image (default extension filter target)
/// /app.js           -> JS file referencing /api/secret/endpoint.json
/// /api/secret/endpoint.json -> JSON leaf
/// /form             -> GET form to /submit with input name=q
/// /submit           -> form target
/// /page?a=1 /page?a=2 -> same path different query
/// /users/1 /users/2 -> similar-URL filter targets
/// /robots.txt /sitemap.xml -> known files
/// /private /from-sitemap -> known-file discoveries
/// ```
pub fn standard_site(external_link: Option<&str>) -> HashMap<String, Route> {
    let mut routes = HashMap::new();
    let external = external_link
        .map(|url| format!("<a href=\"{}\">External</a>", url))
        .unwrap_or_default();
    routes.insert(
        "/".to_string(),
        Route::html(format!(
            "<html><body>\
             <a href=\"/about\">About</a>\
             <a href=\"/contact\">Contact</a>\
             {}\
             <img src=\"/logo.png\">\
             <script src=\"/app.js\"></script>\
             <a href=\"/form\">Form</a>\
             <a href=\"/page?a=1\">Page 1</a>\
             <a href=\"/page?a=2\">Page 2</a>\
             <a href=\"/users/1\">User 1</a>\
             <a href=\"/users/2\">User 2</a>\
             </body></html>",
            external,
        )),
    );
    routes.insert("/other-domain".to_string(), Route::html("<html><body>other host page</body></html>"));
    routes.insert("/about".to_string(), Route::html("<html><body><a href=\"/contact\">Contact</a><h1>About</h1></body></html>"));
    routes.insert("/contact".to_string(), Route::html("<html><body><h1>Contact</h1></body></html>"));
    routes.insert("/logo.png".to_string(), Route { status: 200, content_type: "image/png", body: "PNGDATA".to_string() });
    routes.insert(
        "/app.js".to_string(),
        Route { status: 200, content_type: "application/javascript", body: "console.log(\"/api/secret/endpoint.json\");".to_string() },
    );
    routes.insert(
        "/api/secret/endpoint.json".to_string(),
        Route { status: 200, content_type: "application/json", body: "{\"ok\":true}".to_string() },
    );
    routes.insert(
        "/form".to_string(),
        Route::html("<html><body><form action=\"/submit\" method=\"get\"><input type=\"text\" name=\"q\"><input type=\"submit\"></form></body></html>"),
    );
    routes.insert("/submit".to_string(), Route::text("submitted"));
    routes.insert("/page".to_string(), Route::html("<html><body>page</body></html>"));
    routes.insert("/users/1".to_string(), Route::html("<html><body>user one</body></html>"));
    routes.insert("/users/2".to_string(), Route::html("<html><body>user two</body></html>"));
    routes.insert(
        "/robots.txt".to_string(),
        Route::text("User-agent: *\nDisallow: /private\nSitemap: /sitemap.xml"),
    );
    routes.insert(
        "/sitemap.xml".to_string(),
        Route { status: 200, content_type: "application/xml", body: "<urlset><url><loc>/from-sitemap</loc></url></urlset>".to_string() },
    );
    routes.insert("/private".to_string(), Route::text("private area"));
    routes.insert("/from-sitemap".to_string(), Route::text("from sitemap"));
    routes
}

/// Convenience: build default crawl options pointed at `base_url`.
pub fn crawl_options(base: &str) -> celestia_spider::Options {
    let mut o = celestia_spider::Options::with_defaults();
    o.urls = vec![format!("{}/", base)];
    o.concurrency = 4;
    o.parallelism = 2;
    o
}

// --------------------------------------------------------------------------
// Blocking server (for sync CLI tests that run the binary as a child process)
// --------------------------------------------------------------------------

/// Spawn a blocking (std::thread) HTTP server. Usable from non-async tests:
/// the server threads run for the process lifetime, governed by the shutdown
/// flag on the returned handle.
pub fn spawn_sync(routes: HashMap<String, Route>) -> TestServer {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{}", addr);

    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(AtomicUsize::new(0));
    let routes = Arc::new(routes);

    let requests_thread = Arc::clone(&requests);
    let shutdown_accept = Arc::clone(&shutdown);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if shutdown_accept.load(Ordering::SeqCst) == 1 {
                return;
            }
            let Ok(mut stream) = stream else { continue };
            let routes = Arc::clone(&routes);
            let requests = Arc::clone(&requests_thread);
            std::thread::spawn(move || {
                let mut buf = vec![0u8; 8192];
                let Ok(n) = stream.read(&mut buf) else { return };
                let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                let Some(request_line) = raw.lines().next() else { return };
                let path_with_query = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                requests.lock().unwrap().push(path_with_query.clone());
                let path = path_with_query.split('?').next().unwrap_or("/").to_string();

                let (status, content_type, body) = match routes.get(&path) {
                    Some(r) => (r.status, r.content_type, r.body.clone()),
                    None => (404, "text/plain", "not found".to_string()),
                };
                let reason = if status == 404 { "Not Found" } else { "OK" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            });
        }
    });

    TestServer { base_url, requests, shutdown, addr }
}
