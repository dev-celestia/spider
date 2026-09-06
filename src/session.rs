//! UI-integration facade: spawn a crawl as a managed background task with a
//! serializable config, a broadcast event stream, and cancel/pause/resume control.
//!
//! This is the module UI hosts (Tauri, Electron sidecars, GUI shells) should
//! talk to. The flow is always the same:
//!
//! 1. Build a [`CrawlConfig`] (serde — comes straight from your UI's JSON).
//! 2. [`CrawlSession::spawn`] it — returns a [`CrawlSessionHandle`] immediately.
//! 3. [`CrawlSessionHandle::take_events`] / [`CrawlSessionHandle::subscribe`] for the
//!    [`CrawlerEvent`] stream (forward each event to your UI, e.g. Tauri `emit`).
//! 4. Drive it: [`CrawlSessionHandle::pause`] / [`CrawlSessionHandle::resume`] /
//!    [`CrawlSessionHandle::cancel`].
//! 5. [`CrawlSessionHandle::join`] for the final [`SessionSummary`].
//!
//! Must be called from within an active tokio runtime (Tauri provides one).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::broadcast;

use crate::control::CrawlControl;
use crate::runner::{Runner, RunnerSummary};
use crate::types::events::{CrawlerEvent, SessionSummary};
use crate::types::options::{KnownFiles, Options, Strategy};

/// Serializable, UI-friendly crawl configuration.
///
/// Covers the commonly presented subset of [`Options`]; defaults mirror
/// `Options::with_defaults()`. Deserialize it directly from UI state (Tauri
/// command argument, JSON-RPC param, etc.) with no glue code.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CrawlConfig {
    /// Seed URLs to start the crawl from (required, at least one).
    pub urls: Vec<String>,
    /// Maximum crawl depth; 0 with `crawl_duration_secs == 0` fails validation.
    pub max_depth: i32,
    /// Hard wall-clock limit in seconds; 0 = unlimited.
    pub crawl_duration_secs: u64,
    /// Concurrent fetchers per seed.
    pub concurrency: usize,
    /// Seeds processed concurrently.
    pub parallelism: usize,
    /// Max requests per second.
    pub rate_limit: usize,
    /// Delay between requests in seconds.
    pub delay_secs: u64,
    /// Per-request timeout in seconds.
    pub timeout_secs: u64,
    /// Retries per failed request.
    pub retries: i32,
    /// Visit strategy: breadth-first or depth-first.
    pub strategy: Strategy,
    /// Crawl robots.txt / sitemap.xml.
    pub known_files: KnownFiles,
    /// Headless Chrome rendering (JS execution).
    pub headless: bool,
    /// Headless Chrome for pages, plain HTTP for static sub-resources.
    pub headless_hybrid: bool,
    /// Anti-bot stealth mode.
    pub stealth: bool,
    /// Show the Chrome window (useful while demoing a UI-driven crawl).
    pub show_browser: bool,
    /// In-scope URL regexes.
    pub scope: Vec<String>,
    /// Out-of-scope URL regexes.
    pub out_of_scope: Vec<String>,
    /// Excluded host filters.
    pub exclude: Vec<String>,
    /// Extra headers attached to every request.
    pub custom_headers: HashMap<String, String>,
    /// HTTP/SOCKS5 proxy URL.
    pub proxy: String,
    /// Parse JavaScript responses for endpoints.
    pub scrape_js_responses: bool,
    /// Detect technologies per response.
    pub tech_detect: bool,
    /// Attach extracted forms to each result.
    pub form_extraction: bool,
    /// Experimental automatic form filling.
    pub automatic_form_fill: bool,
    /// Knowledge-base classification.
    pub knowledge_base: bool,
    /// Knowledge-base secrets extraction.
    pub secrets: bool,
    /// Knowledge-base REST/GraphQL endpoint extraction.
    pub endpoints: bool,
    /// Filter similar-looking URLs.
    pub filter_similar: bool,
    /// Ignore query params when deduplicating URLs.
    pub ignore_query_params: bool,
    /// Cap pages per domain; 0 = unlimited.
    pub max_domain_pages: usize,
    /// Write JSONL results to this file (in addition to the event stream).
    pub output_file: String,
    /// Suppress library log lines on stderr (recommended for UI hosts).
    pub silent: bool,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        let d = Options::with_defaults();
        CrawlConfig {
            urls: Vec::new(),
            max_depth: d.max_depth,
            crawl_duration_secs: 0,
            concurrency: d.concurrency,
            parallelism: d.parallelism,
            rate_limit: d.rate_limit,
            delay_secs: d.delay,
            timeout_secs: d.timeout,
            retries: d.retries,
            strategy: d.strategy,
            known_files: KnownFiles::None,
            headless: false,
            headless_hybrid: false,
            stealth: d.stealth,
            show_browser: false,
            scope: Vec::new(),
            out_of_scope: Vec::new(),
            exclude: Vec::new(),
            custom_headers: HashMap::new(),
            proxy: String::new(),
            scrape_js_responses: false,
            tech_detect: false,
            form_extraction: false,
            automatic_form_fill: false,
            knowledge_base: false,
            secrets: false,
            endpoints: false,
            filter_similar: false,
            ignore_query_params: false,
            max_domain_pages: 0,
            output_file: String::new(),
            silent: true,
        }
    }
}

impl CrawlConfig {
    /// Convert into validated, engine-ready [`Options`].
    pub fn to_options(&self) -> Result<Options, String> {
        if self.urls.is_empty() {
            return Err("CrawlConfig requires at least one seed URL".to_string());
        }
        if self.headless && self.headless_hybrid {
            return Err("headless and headless_hybrid are mutually exclusive".to_string());
        }

        let mut o = Options::with_defaults();
        o.urls = self.urls.clone();
        o.max_depth = self.max_depth;
        o.crawl_duration = Duration::from_secs(self.crawl_duration_secs);
        o.concurrency = self.concurrency;
        o.parallelism = self.parallelism;
        o.rate_limit = self.rate_limit;
        o.delay = self.delay_secs;
        o.timeout = self.timeout_secs;
        o.retries = self.retries;
        o.strategy = self.strategy;
        o.known_files = self.known_files;
        o.headless = self.headless;
        o.headless_hybrid = self.headless_hybrid;
        o.stealth = self.stealth;
        o.show_browser = self.show_browser;
        o.scope = self.scope.clone();
        o.out_of_scope = self.out_of_scope.clone();
        o.exclude = self.exclude.clone();
        o.custom_headers = self.custom_headers.clone();
        o.proxy = self.proxy.clone();
        o.scrape_js_responses = self.scrape_js_responses;
        o.tech_detect = self.tech_detect;
        o.form_extraction = self.form_extraction;
        o.automatic_form_fill = self.automatic_form_fill;
        o.knowledge_base = self.knowledge_base;
        o.secrets = self.secrets;
        o.endpoints = self.endpoints;
        o.filter_similar = self.filter_similar;
        o.ignore_query_params = self.ignore_query_params;
        o.max_domain_pages = self.max_domain_pages;
        o.output_file = self.output_file.clone();
        o.silent = self.silent;
        o.json = !self.output_file.is_empty();
        o.validate()?;
        Ok(o)
    }
}

/// Live counters shared between the result-forwarding callbacks and
/// [`CrawlerEvent::Progress`] emission.
#[derive(Default)]
struct ProgressCounters {
    results: AtomicUsize,
    skipped: AtomicUsize,
    failed: AtomicUsize,
}

impl ProgressCounters {
    fn snapshot(&self) -> (usize, usize, usize) {
        (
            self.results.load(Ordering::Relaxed),
            self.skipped.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
        )
    }
}

/// Lifecycle phase of a crawl session, as reported by [`CrawlSessionHandle::snapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// Accepting and processing work.
    Running,
    /// Paused via the handle; workers idle until resume or cancel.
    Paused,
    /// Cancellation requested; draining current work.
    Cancelling,
    /// Crawl task has completed.
    Finished,
}

/// Serializable point-in-time view of a session, for UI hydration.
///
/// The companion to the event stream: a UI that joins late (window reopen,
/// reconnect) calls `snapshot()` once to load current counters and phase, then
/// subscribes to [`CrawlerEvent`]s for live updates.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    /// Seed URLs the session was spawned with.
    pub seeds: Vec<String>,
    /// Engine selected for the crawl: `standard`, `headless`, or `hybrid`.
    pub engine: String,
    /// Current lifecycle phase.
    pub phase: SessionPhase,
    /// Successful results so far.
    pub results: usize,
    /// URLs skipped by filters so far.
    pub skipped: usize,
    /// Failed fetches so far.
    pub failed: usize,
}

/// A spawned, managed crawl. Clone-free handle; hold it in your app state.
pub struct CrawlSessionHandle {
    control: Arc<CrawlControl>,
    events: broadcast::Sender<CrawlerEvent>,
    /// Pre-subscribed receiver so the first consumer never misses `Started`.
    initial_rx: Mutex<Option<broadcast::Receiver<CrawlerEvent>>>,
    task: Mutex<Option<tokio::task::JoinHandle<Result<RunnerSummary, String>>>>,
    counters: Arc<ProgressCounters>,
    seeds: Vec<String>,
    engine: String,
}

impl CrawlSessionHandle {
    /// Take the receiver pre-subscribed before the crawl started (guaranteed to
    /// observe the `Started` event). Subsequent consumers use [`Self::subscribe`].
    pub fn take_events(&self) -> Option<broadcast::Receiver<CrawlerEvent>> {
        self.initial_rx.lock().unwrap().take()
    }

    /// Subscribe an additional event consumer (e.g. a second window or a log file).
    pub fn subscribe(&self) -> broadcast::Receiver<CrawlerEvent> {
        self.events.subscribe()
    }

    /// Request cooperative cancellation. The current queue drains and the
    /// finished summary reports `cancelled: true`.
    pub fn cancel(&self) {
        if !self.control.is_cancelled() {
            self.control.cancel();
            let _ = self.events.send(CrawlerEvent::Cancelled);
        }
    }

    /// Pause the crawl: fetchers idle until resumed (or cancelled).
    pub fn pause(&self) {
        if !self.control.is_paused() && !self.control.is_cancelled() {
            self.control.pause();
            let _ = self.events.send(CrawlerEvent::Paused);
        }
    }

    /// Resume a paused crawl.
    pub fn resume(&self) {
        if self.control.is_paused() {
            self.control.resume();
            let _ = self.events.send(CrawlerEvent::Resumed);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.control.is_paused()
    }

    pub fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }

    /// Point-in-time view of the session: phase, seeds, engine, and live
    /// counters. Use it to hydrate a UI that joins after the crawl started,
    /// then subscribe to events for incremental updates.
    pub fn snapshot(&self) -> SessionSnapshot {
        let task_finished = self
            .task
            .lock()
            .unwrap()
            .as_ref()
            .map(|t| t.is_finished())
            .unwrap_or(true);
        let phase = if task_finished {
            SessionPhase::Finished
        } else if self.control.is_cancelled() {
            SessionPhase::Cancelling
        } else if self.control.is_paused() {
            SessionPhase::Paused
        } else {
            SessionPhase::Running
        };
        let (results, skipped, failed) = self.counters.snapshot();
        SessionSnapshot {
            seeds: self.seeds.clone(),
            engine: self.engine.clone(),
            phase,
            results,
            skipped,
            failed,
        }
    }

    /// Await completion, returning the crawl summary (or the crawl's error).
    pub async fn join(self) -> Result<SessionSummary, String> {
        let task = self.task.lock().unwrap().take();
        match task {
            Some(task) => match task.await {
                Ok(Ok(summary)) => Ok(SessionSummary::from(&summary)),
                Ok(Err(e)) => Err(e),
                Err(e) => Err(format!("crawl task panicked: {e}")),
            },
            None => Err("crawl task already joined".to_string()),
        }
    }
}

/// Namespace facade for starting a crawl session: [`CrawlSession::spawn`].
pub struct CrawlSession;

impl CrawlSession {
    /// Spawn a crawl session from a UI-supplied config. Must run inside a tokio runtime.
    pub fn spawn(config: CrawlConfig) -> Result<CrawlSessionHandle, String> {
        spawn(config)
    }
}

/// Spawn a crawl session from a UI-supplied config. Must run inside a tokio runtime.
pub fn spawn(config: CrawlConfig) -> Result<CrawlSessionHandle, String> {
    let (events, _) = broadcast::channel(1024);
    let initial_rx = events.subscribe();

    let mut options = config.to_options()?;
    let control = Arc::new(CrawlControl::default());

    // Forward results/skips onto the event channel with live Progress counters.
    let counters = Arc::new(ProgressCounters::default());
    let tx = events.clone();
    let c = Arc::clone(&counters);
    options.on_result = Some(Box::new(move |result| {
        if result.error.is_empty() {
            c.results.fetch_add(1, Ordering::Relaxed);
            let (status_code, content_length, depth, forms, technologies) = result
                .response
                .as_ref()
                .map(|r| {
                    (
                        r.status_code,
                        r.content_length,
                        r.depth,
                        r.forms.len(),
                        r.technologies.clone(),
                    )
                })
                .unwrap_or((0, 0, 0, 0, Vec::new()));
            let _ = tx.send(CrawlerEvent::PageFetched {
                url: result.url().to_string(),
                status_code,
                depth,
                content_length,
                forms,
                technologies,
            });
        } else {
            c.failed.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send(CrawlerEvent::PageError {
                url: result.url().to_string(),
                error: result.error.clone(),
            });
        }
        let _ = tx.send(CrawlerEvent::Progress {
            results: c.results.load(Ordering::Relaxed),
            skipped: c.skipped.load(Ordering::Relaxed),
            failed: c.failed.load(Ordering::Relaxed),
        });
    }));
    let tx = events.clone();
    let c = Arc::clone(&counters);
    options.on_skip_url = Some(Box::new(move |url| {
        c.skipped.fetch_add(1, Ordering::Relaxed);
        let _ = tx.send(CrawlerEvent::PageSkipped {
            url: url.to_string(),
            reason: "filtered".to_string(),
        });
        let _ = tx.send(CrawlerEvent::Progress {
            results: c.results.load(Ordering::Relaxed),
            skipped: c.skipped.load(Ordering::Relaxed),
            failed: c.failed.load(Ordering::Relaxed),
        });
    }));

    let mut runner = Runner::with_control(options, Arc::clone(&control))?;
    runner.set_event_channel(events.clone());
    let task = tokio::spawn(async move { runner.run().await });

    let engine = if config.headless {
        "headless"
    } else if config.headless_hybrid {
        "hybrid"
    } else {
        "standard"
    };

    Ok(CrawlSessionHandle {
        control,
        events,
        initial_rx: Mutex::new(Some(initial_rx)),
        task: Mutex::new(Some(task)),
        counters,
        seeds: config.urls.clone(),
        engine: engine.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_json_round_trip() {
        let config = CrawlConfig {
            urls: vec!["https://example.com".into()],
            max_depth: 2,
            headless: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"maxDepth\":2"));
        let back: CrawlConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn test_config_defaults_camel_case_deserialize() {
        let config: CrawlConfig =
            serde_json::from_str(r#"{ "urls": ["https://example.com"], "scrapeJsResponses": true }"#)
                .unwrap();
        assert_eq!(config.max_depth, 3);
        assert!(config.scrape_js_responses);
        assert!(config.silent, "silent defaults to true for UI hosts");
    }

    #[test]
    fn test_config_requires_urls() {
        assert!(CrawlConfig::default().to_options().is_err());
    }

    #[test]
    fn test_config_to_options() {
        let config = CrawlConfig {
            urls: vec!["https://example.com".into()],
            max_depth: 5,
            concurrency: 4,
            scope: vec!["/docs".into()],
            ..Default::default()
        };
        let options = config.to_options().unwrap();
        assert_eq!(options.max_depth, 5);
        assert_eq!(options.concurrency, 4);
        assert_eq!(options.scope, vec!["/docs".to_string()]);
        assert!(options.silent);
    }

    #[test]
    fn test_config_rejects_conflicting_engines() {
        let config = CrawlConfig {
            urls: vec!["https://example.com".into()],
            headless: true,
            headless_hybrid: true,
            ..Default::default()
        };
        assert!(config.to_options().is_err());
    }

    #[tokio::test]
    async fn test_session_lifecycle_offline() {
        // Connection-refused target: session still runs to completion, emitting
        // Started / PageError / Progress / Finished without any network.
        let config = CrawlConfig {
            urls: vec!["http://127.0.0.1:1/".into()],
            max_depth: 1,
            retries: 0,
            timeout_secs: 2,
            ..Default::default()
        };
        let session = spawn(config).unwrap();
        let mut rx = session.take_events().unwrap();

        let mut saw_started = false;
        let mut saw_finished = false;
        let mut finished_summary = None;
        while let Ok(event) = rx.recv().await {
            match event {
                CrawlerEvent::Started { engine, .. } => {
                    saw_started = true;
                    assert_eq!(engine, "standard");
                }
                CrawlerEvent::Finished { summary } => {
                    finished_summary = Some(summary);
                    saw_finished = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(saw_started, "Started emitted");
        assert!(saw_finished, "Finished emitted");
        let summary = finished_summary.unwrap();
        assert_eq!(summary.failed, 1);
        assert!(!summary.cancelled);
        let final_summary = session.join().await.unwrap();
        assert_eq!(final_summary.failed, 1);
    }

    #[tokio::test]
    async fn test_session_snapshot_lifecycle() {
        let config = CrawlConfig {
            urls: vec!["http://127.0.0.1:1/".into()],
            max_depth: 1,
            retries: 0,
            timeout_secs: 2,
            ..Default::default()
        };
        let session = spawn(config).unwrap();

        // Hydration view before any event: seeds + engine + running phase.
        let snap = session.snapshot();
        assert_eq!(snap.seeds, vec!["http://127.0.0.1:1/".to_string()]);
        assert_eq!(snap.engine, "standard");
        assert_eq!(snap.phase, SessionPhase::Running);
        assert_eq!((snap.results, snap.skipped, snap.failed), (0, 0, 0));

        // Pause is observable without events.
        session.pause();
        assert_eq!(session.snapshot().phase, SessionPhase::Paused);
        session.resume();

        // Wait for completion, then the phase flips to Finished with counters.
        let mut rx = session.subscribe();
        while let Ok(event) = rx.recv().await {
            if matches!(event, CrawlerEvent::Finished { .. }) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        let snap = session.snapshot();
        assert_eq!(snap.phase, SessionPhase::Finished);
        assert_eq!(snap.failed, 1);

        let summary = session.join().await.unwrap();
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn test_session_snapshot_serializes_camel_case() {
        let snap = SessionSnapshot {
            seeds: vec!["https://example.com".into()],
            engine: "standard".into(),
            phase: SessionPhase::Paused,
            results: 1,
            skipped: 2,
            failed: 3,
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["phase"], "paused");
        assert!(json.get("seeds").is_some());
        assert_eq!(json["failed"], 3);
    }

    #[tokio::test]
    async fn test_session_cancel_finishes_promptly() {
        let config = CrawlConfig {
            urls: vec!["http://127.0.0.1:1/".into()],
            max_depth: 1,
            retries: 0,
            timeout_secs: 10,
            ..Default::default()
        };
        let session = spawn(config).unwrap();
        session.cancel();
        assert!(session.is_cancelled());
        let summary = tokio::time::timeout(Duration::from_secs(10), session.join())
            .await
            .expect("cancel must finish promptly")
            .unwrap();
        assert!(summary.cancelled);
    }

    #[tokio::test]
    async fn test_session_pause_resume_events() {
        let config = CrawlConfig {
            urls: vec!["http://127.0.0.1:1/".into()],
            max_depth: 1,
            retries: 0,
            timeout_secs: 5,
            ..Default::default()
        };
        let session = spawn(config).unwrap();
        let mut rx = session.subscribe();
        session.pause();
        assert!(session.is_paused());
        session.resume();
        assert!(!session.is_paused());
        let mut saw_paused = false;
        let mut saw_resumed = false;
        while let Ok(event) = rx.recv().await {
            match event {
                CrawlerEvent::Paused => saw_paused = true,
                CrawlerEvent::Resumed => saw_resumed = true,
                CrawlerEvent::Finished { .. } => break,
                _ => {}
            }
        }
        assert!(saw_paused && saw_resumed);
        let _ = session.join().await;
    }
}
