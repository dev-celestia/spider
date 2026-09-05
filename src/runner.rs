//! Port of the reference crawler `internal/runner` — option validation, input parsing
//! (args + stdin), engine execution, ctrl-c handling, health check, and the
//! crawl summary.

use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::engine;
use crate::output::{list_output_fields, log, LogLevel, StandardWriter};
use crate::types::options::Options;

/// Runner wraps validated options and drives the crawl (reference crawler `runner.Runner`).
pub struct Runner {
    pub options: Arc<Options>,
    cancel: Arc<AtomicBool>,
}

impl Runner {
    /// Create a runner, validating options (reference crawler `runner.New`).
    pub fn new(mut options: Options) -> Result<Runner, String> {
        options.validate()?;
        Ok(Runner {
            options: Arc::new(options),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Run the crawl over all inputs (reference crawler `Runner.ExecuteCrawl`).
    pub async fn run(&mut self) -> Result<RunnerSummary, String> {
        let seeds = self.parse_inputs();
        if seeds.is_empty() {
            return Err("no inputs specified for crawler".to_string());
        }

        // Ctrl-C handler: cooperative cancellation + resume state save.
        let cancel = Arc::clone(&self.cancel);
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                log(LogLevel::Info, "received interrupt signal, saving state and stopping...");
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });

        let writer = Arc::new(StandardWriter::from_options(&self.options));

        // List output fields mode (-lof) exits after listing.
        if self.options.list_output_fields {
            list_output_fields();
            return Ok(RunnerSummary::default());
        }

        if self.options.health_check {
            health_check();
        }

        if !self.options.resolvers.is_empty() {
            log(
                LogLevel::Warning,
                "custom resolvers are accepted but not yet enforced by the Rust HTTP stack",
            );
        }
        if self.options.tls_impersonate {
            log(
                LogLevel::Warning,
                "tls-impersonate is accepted but only best-effort in the Rust HTTP stack",
            );
        }
        if self.options.pprof_server {
            log(LogLevel::Warning, "pprof-server is a Go-runtime feature and is a no-op here");
        }

        let start = std::time::Instant::now();
        let crawlers = engine::execute(Arc::clone(&self.options), writer, Arc::clone(&self.cancel), seeds)
            .await;

        let mut summary = RunnerSummary::default();
        for crawler in &crawlers {
            summary.failed += crawler.stats.failed.load(std::sync::atomic::Ordering::SeqCst);
            summary.results += crawler.stats.results.load(std::sync::atomic::Ordering::SeqCst);
            summary.skipped += crawler.stats.skipped.load(std::sync::atomic::Ordering::SeqCst);
            summary
                .visited_urls
                .extend(crawler.stats.visited_urls.lock().unwrap().iter().cloned());
        }
        summary.duration = start.elapsed();
        summary.cancelled = self.cancel.load(std::sync::atomic::Ordering::SeqCst);

        log(
            LogLevel::Info,
            &format!(
                "crawl finished: {} results, {} skipped, {} failed in {:?}",
                summary.results, summary.skipped, summary.failed, summary.duration
            ),
        );
        Ok(summary)
    }

    /// Parse and dedupe inputs from options URLs and stdin
    /// (reference crawler `parseInputs`).
    fn parse_inputs(&self) -> Vec<String> {
        let mut values = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for url in &self.options.urls {
            let value = normalize_input(url);
            if !value.is_empty() && seen.insert(value.clone()) {
                values.push(value);
            }
        }

        if self.options.urls_from_stdin {
            let mut buffer = String::new();
            if std::io::stdin().read_to_string(&mut buffer).is_ok() {
                for line in buffer.lines() {
                    let value = normalize_input(line);
                    if !value.is_empty() && seen.insert(value.clone()) {
                        values.push(value);
                    }
                }
            }
        }
        values
    }
}

/// Summary of a completed run.
#[derive(Debug, Default, Clone)]
pub struct RunnerSummary {
    pub results: usize,
    pub skipped: usize,
    pub failed: usize,
    pub visited_urls: Vec<String>,
    pub duration: std::time::Duration,
    pub cancelled: bool,
}

/// Normalize an input line (reference crawler `normalizeInput`).
fn normalize_input(value: &str) -> String {
    value.trim().to_string()
}

/// Self diagnostic check (reference crawler `-hc` health-check).
pub fn health_check() {
    println!("Health Check:");
    println!("  version:        {}", version());
    println!("  rust:           {}", rustc_version_or_unknown());

    // DNS / network probe.
    let dns_ok = std::net::ToSocketAddrs::to_socket_addrs("example.com:443")
        .map(|mut a| a.next().is_some())
        .unwrap_or(false);
    println!("  dns resolution: {}", ok(dns_ok));

    // Chrome availability for headless mode.
    let chrome = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).exists());
    println!("  chrome:         {}", ok(chrome));

    // Outbound HTTPS probe (async runtime not yet entered here — TCP probe).
    let tcp_ok = std::net::TcpStream::connect_timeout(
        &"example.com:443".parse().unwrap_or_else(|_| std::net::SocketAddr::from(([1, 1, 1, 1], 443))),
        std::time::Duration::from_secs(5),
    )
    .is_ok();
    println!("  network (tcp):  {}", ok(tcp_ok));
}

fn ok(b: bool) -> &'static str {
    if b { "ok" } else { "failed" }
}

fn rustc_version_or_unknown() -> String {
    option_env!("RUSTC_VERSION").unwrap_or("unknown").to_string()
}

/// Crate version for the banner/version flag.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_requires_depth_or_duration() {
        let mut o = Options::with_defaults();
        o.max_depth = 0;
        assert!(Runner::new(o).is_err());
    }

    #[test]
    fn test_runner_requires_inputs() {
        let o = Options::with_defaults();
        let r = Runner::new(o).unwrap();
        assert!(r.parse_inputs().is_empty());
    }

    #[test]
    fn test_normalize_input() {
        assert_eq!(normalize_input("  https://x.com \n"), "https://x.com");
    }

    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }
}
