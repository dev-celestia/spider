//! Port of the reference crawler `internal/runner` — option validation, input parsing
//! (args + stdin), engine execution, ctrl-c handling, health check, and the
//! crawl summary.
//!
//! UI hosts should use [`Runner::with_control`] (no signal handler, shared
//! [`CrawlControl`]) plus [`Runner::set_event_channel`] to receive
//! [`CrawlerEvent`]s instead of scraping stdout.

use std::collections::HashSet;
use std::io::Read;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::control::CrawlControl;
use crate::engine;
use crate::output::{list_output_fields, log, LogLevel, StandardWriter};
use crate::types::events::{CrawlerEvent, SessionSummary};
use crate::types::options::Options;

/// In-flight seed URLs shared with the engine for resume-state saving
/// (reference crawler `RunnerState.InFlightUrls`).
pub type InFlightUrls = Arc<Mutex<HashSet<String>>>;

/// Default directory for auto-generated resume files (reference crawler
/// uses `~/.config/katana`; the Rust port uses `~/.config/celestia-browser`).
fn default_resume_dir() -> Option<std::path::PathBuf> {
    std::env::home_dir()
        .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))
        .map(|h| h.join(".config").join("celestia-browser"))
}

/// Auto-generated resume file path used when the crawl is interrupted.
fn default_resume_filename() -> String {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let dir = default_resume_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    dir.join(format!("resume-{id}.cfg"))
        .to_string_lossy()
        .to_string()
}

/// Runner wraps validated options and drives the crawl (reference crawler `runner.Runner`).
pub struct Runner {
    pub options: Arc<Options>,
    control: Arc<CrawlControl>,
    events: Option<broadcast::Sender<CrawlerEvent>>,
    /// Seed URLs currently being crawled (for resume-state saving).
    in_flight: InFlightUrls,
    /// Path the interrupt handler would write the resume state to.
    resume_save_path: String,
    /// Whether this runner owns the process-wide ctrl-C handler. UI hosts
    /// disable it: the signal would otherwise fire on their own window too.
    handle_signals: bool,
}

impl Runner {
    /// Create a runner, validating options (reference crawler `runner.New`).
    ///
    /// The runner installs its own ctrl-C handler and owns its cancellation
    /// token. For UI embedding use [`Runner::with_control`] instead.
    pub fn new(mut options: Options) -> Result<Runner, String> {
        // Resume: replace the input list with the saved in-flight URLs
        // (reference crawler `options.ShouldResume` + `RunnerState` load).
        if !options.resume.is_empty() {
            log(LogLevel::Info, "Resuming from save checkpoint");
            let urls = load_resume_state(&options.resume)?;
            if !urls.is_empty() {
                options.urls = urls;
                options.urls_from_stdin = false;
            }
        }
        options.validate()?;
        Ok(Runner {
            options: Arc::new(options),
            control: Arc::new(CrawlControl::default()),
            events: None,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            resume_save_path: default_resume_filename(),
            handle_signals: true,
        })
    }

    /// Create a runner driven by an externally-owned [`CrawlControl`] — the
    /// UI-integration constructor. No ctrl-C handler is installed; the host
    /// application decides when (and whether) to cancel, pause, or resume.
    pub fn with_control(mut options: Options, control: Arc<CrawlControl>) -> Result<Runner, String> {
        options.validate()?;
        Ok(Runner {
            options: Arc::new(options),
            control,
            events: None,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            resume_save_path: default_resume_filename(),
            handle_signals: false,
        })
    }

    /// Attach a `tokio::sync::broadcast` event channel. Every lifecycle and
    /// result event is sent on it; [`Runner::run`] also emits
    /// [`CrawlerEvent::Started`] and [`CrawlerEvent::Finished`].
    pub fn set_event_channel(&mut self, events: broadcast::Sender<CrawlerEvent>) {
        self.events = Some(events);
    }

    /// Shared control handle for cancelling/pausing from outside the task.
    pub fn control(&self) -> Arc<CrawlControl> {
        Arc::clone(&self.control)
    }

    /// Remove the auto-generated resume file after a successful run
    /// (reference crawler main.go removes it post-crawl).
    pub fn remove_resume_file(&self) {
        let path = std::path::Path::new(&self.resume_save_path);
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Send an event to subscribers, ignoring absent receivers.
    fn emit(&self, event: CrawlerEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(event);
        }
    }

    /// Run the crawl over all inputs (reference crawler `Runner.ExecuteCrawl`).
    pub async fn run(&mut self) -> Result<RunnerSummary, String> {
        // List output fields mode (-lof) prints and exits before any input
        // validation (reference crawler handles it first in main).
        if self.options.list_output_fields {
            list_output_fields();
            return Ok(RunnerSummary::default());
        }
        if self.options.health_check {
            health_check();
            return Ok(RunnerSummary::default());
        }

        let seeds = self.parse_inputs();
        if seeds.is_empty() {
            return Err("no inputs specified for crawler".to_string());
        }

        // Ctrl-C handler: cooperative cancellation + resume state save. UI
        // hosts bypass this via `with_control`.
        if self.handle_signals {
            let control = Arc::clone(&self.control);
            let in_flight = Arc::clone(&self.in_flight);
            let save_path = self.resume_save_path.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    log(LogLevel::Info, "received interrupt signal, saving state and stopping...");
                    save_resume_state(&save_path, &in_flight);
                    control.cancel();
                }
            });
        }

        // Networkpolicy-style input filtering (-exclude): cdn is a no-op in
        // the reference crawler too; private-ips/CIDR/port/regex deny lists.
        let exclude = ExcludeFilter::new(&self.options.exclude);
        let seeds: Vec<String> = seeds
            .into_iter()
            .filter(|input| {
                if exclude.validate(input) {
                    true
                } else {
                    log(LogLevel::Info, &format!("Skipping excluded host {input}"));
                    false
                }
            })
            .map(|input| add_scheme_if_not_exists(&input))
            .collect();
        if seeds.is_empty() {
            return Err("no inputs specified for crawler".to_string());
        }

        let writer = Arc::new(StandardWriter::from_options(&self.options));
        let writer_count = Arc::clone(&writer);

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

        let engine_name = if self.options.headless {
            "headless"
        } else if self.options.headless_hybrid {
            "hybrid"
        } else {
            "standard"
        };
        self.emit(CrawlerEvent::Started {
            seeds: seeds.clone(),
            engine: engine_name.to_string(),
        });

        let start = std::time::Instant::now();
        let crawlers = engine::execute(
            Arc::clone(&self.options),
            writer,
            Arc::clone(&self.control),
            seeds,
            Arc::clone(&self.in_flight),
        )
        .await;
        self.in_flight.lock().unwrap().clear();

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
        summary.cancelled = self.control.is_cancelled();

        // Completion stats (reference crawler showCompletionStats).
        if self.options.content_similarity_enabled() {
            let mut sim = crate::utils::filters::SimilarityStats::default();
            let mut mode = "simhash";
            for crawler in &crawlers {
                let st = crawler.similarity.stats();
                sim.processed += st.processed;
                sim.accepted += st.accepted;
                sim.filtered += st.filtered;
                mode = crawler.similarity.mode_name();
            }
            if sim.processed > 0 {
                let rate = sim.filtered as f64 / sim.processed as f64 * 100.0;
                log(
                    LogLevel::Info,
                    &format!(
                        "Content similarity ({}): {} processed, {} accepted, {} filtered - {:.1}% filter rate",
                        mode, sim.processed, sim.accepted, sim.filtered, rate
                    ),
                );
            }
        }
        log(
            LogLevel::Info,
            &format!(
                "Crawl completed in {:?}. {} endpoints found.",
                summary.duration,
                writer_count.result_count()
            ),
        );
        log(
            LogLevel::Info,
            &format!(
                "crawl finished: {} results, {} skipped, {} failed",
                summary.results, summary.skipped, summary.failed
            ),
        );
        self.emit(CrawlerEvent::Finished {
            summary: SessionSummary::from(&summary),
        });
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

impl From<&RunnerSummary> for SessionSummary {
    fn from(s: &RunnerSummary) -> Self {
        SessionSummary {
            results: s.results,
            skipped: s.skipped,
            failed: s.failed,
            visited_urls: s.visited_urls.clone(),
            duration_ms: s.duration.as_millis() as u64,
            cancelled: s.cancelled,
        }
    }
}

/// Delete auto-generated resume files older than `days` days
/// (reference crawler `cleanupOldResumeFiles`, 10-day retention).
pub fn cleanup_old_resume_files(days: u64) {
    let Some(dir) = default_resume_dir() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let cutoff = std::time::Duration::from_secs(days * 86_400);
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("resume-") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age > cutoff {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

/// Normalize an input line (reference crawler `normalizeInput`).
fn normalize_input(value: &str) -> String {
    value.trim().to_string()
}

/// Load a resume state file: `{"in_flight_urls": [...]}` written by this port,
/// or the reference crawler's `{"InFlightUrls": {...}}` object form.
fn load_resume_state(path: &str) -> Result<Vec<String>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("could not read resume file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("could not parse resume file: {e}"))?;
    let mut urls = Vec::new();
    match value.get("in_flight_urls").or_else(|| value.get("InFlightUrls")) {
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(u) = item.as_str() {
                    urls.push(u.to_string());
                }
            }
        }
        Some(serde_json::Value::Object(map)) => {
            for key in map.keys() {
                urls.push(key.clone());
            }
        }
        _ => {}
    }
    Ok(urls)
}

/// Write the in-flight seed URLs to a resume file (`Runner.SaveState`).
fn save_resume_state(path: &str, in_flight: &InFlightUrls) {
    let urls: Vec<String> = in_flight.lock().unwrap().iter().cloned().collect();
    let state = serde_json::json!({ "in_flight_urls": urls });
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(err) = std::fs::write(path, serde_json::to_string(&state).unwrap_or_default()) {
        log(LogLevel::Warning, &format!("Couldn't create resume file: {err}"));
    }
}

/// Add a scheme to scheme-less input URLs (reference crawler
/// `addSchemeIfNotExists`): `example.com` → `https://example.com`, with
/// `http` for explicit `:80`/`:8080` ports.
pub fn add_scheme_if_not_exists(input: &str) -> String {
    let input = input.trim();
    if input.starts_with("http://") || input.starts_with("https://") {
        return input.to_string();
    }
    if let Ok(parsed) = url::Url::parse(&format!("//{input}")) {
        if parsed.port() == Some(80) || parsed.port() == Some(8080) {
            return format!("http://{input}");
        }
    }
    format!("https://{input}")
}

/// Input exclusion filter (reference crawler `-exclude` + networkpolicy):
/// `cdn` is a no-op there too; `private-ips` denies private ranges; CIDR
/// entries deny ranges; bare ports deny ports; anything else is a regex.
struct ExcludeFilter {
    cidrs: Vec<(std::net::IpAddr, u32)>,
    ips: Vec<std::net::IpAddr>,
    ports: Vec<u16>,
    regexes: Vec<regex::Regex>,
}

impl ExcludeFilter {
    fn new(entries: &[String]) -> Self {
        let mut filter = ExcludeFilter {
            cidrs: Vec::new(),
            ips: Vec::new(),
            ports: Vec::new(),
            regexes: Vec::new(),
        };
        for entry in entries {
            let entry = entry.trim();
            match entry {
                "cdn" | "" => {}
                "private-ips" => {
                    for cidr in ["0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8",
                                 "169.254.0.0/16", "172.16.0.0/12", "192.0.0.0/24",
                                 "192.168.0.0/16", "198.18.0.0/15", "::1/128", "fc00::/7", "fe80::/10"]
                    {
                        if let Some(c) = parse_cidr(cidr) {
                            filter.cidrs.push(c);
                        }
                    }
                }
                _ => {
                    if let Some(c) = parse_cidr(entry) {
                        filter.cidrs.push(c);
                    } else if let Ok(ip) = entry.parse::<std::net::IpAddr>() {
                        filter.ips.push(ip);
                    } else if let Ok(port) = entry.parse::<u16>() {
                        filter.ports.push(port);
                    } else if let Ok(re) = regex::Regex::new(entry) {
                        filter.regexes.push(re);
                    }
                }
            }
        }
        filter
    }

    /// Whether an input URL passes the deny list (true = allowed).
    fn validate(&self, input: &str) -> bool {
        for re in &self.regexes {
            if re.is_match(input) {
                return false;
            }
        }
        let Ok(parsed) = url::Url::parse(input) else {
            return true;
        };
        if let Some(port) = parsed.port() {
            if self.ports.contains(&port) {
                return false;
            }
        }
        let Some(host) = parsed.host_str() else {
            return true;
        };
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            if self.ips.contains(&ip) {
                return false;
            }
            for (net, prefix) in &self.cidrs {
                if ip_in_cidr(ip, *net, *prefix) {
                    return false;
                }
            }
        }
        true
    }
}

fn parse_cidr(entry: &str) -> Option<(std::net::IpAddr, u32)> {
    let (addr, prefix) = entry.split_once('/')?;
    let addr: std::net::IpAddr = addr.parse().ok()?;
    let prefix: u32 = prefix.parse().ok()?;
    Some((addr, prefix))
}

fn ip_in_cidr(ip: std::net::IpAddr, net: std::net::IpAddr, prefix: u32) -> bool {
    match (ip, net) {
        (std::net::IpAddr::V4(ip), std::net::IpAddr::V4(net)) => {
            let ip = u32::from(ip);
            let net = u32::from(net);
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix.min(32)) };
            ip & mask == net & mask
        }
        (std::net::IpAddr::V6(ip), std::net::IpAddr::V6(net)) => {
            let ip = u128::from(ip);
            let net = u128::from(net);
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix.min(128)) };
            ip & mask == net & mask
        }
        _ => false,
    }
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
