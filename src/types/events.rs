//! UI-facing crawl events (`CrawlerEvent`) and completion summary (`SessionSummary`).
//!
//! Every event is `serde`-serializable and `Clone`, designed to be fanned out over a
//! `tokio::sync::broadcast` channel and forwarded verbatim to UI frameworks — e.g. a
//! Tauri `Window::emit`, an Electron IPC message, or a WebSocket frame.

use serde::{Deserialize, Serialize};

/// Completion summary delivered with [`CrawlerEvent::Finished`].
///
/// Field names serialize camelCase so the JS side of a UI bridge (Tauri invoke,
/// Electron IPC) sees one consistent convention across config and events.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// URLs that produced a successful result.
    pub results: usize,
    /// URLs skipped by scope/filters/dedup.
    pub skipped: usize,
    /// URLs that failed to fetch (after retries).
    pub failed: usize,
    /// All URLs that produced a successful result.
    pub visited_urls: Vec<String>,
    /// Wall-clock duration of the crawl in milliseconds.
    pub duration_ms: u64,
    /// Whether the crawl ended because cancellation was requested.
    pub cancelled: bool,
}

/// A single observable occurrence during a crawl session.
///
/// Event names (`type` tag) are `snake_case` (`page_fetched`); event *fields* are
/// `camelCase` (`statusCode`) so JS consumers see one field convention end to end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum CrawlerEvent {
    /// Emitted once when the crawl session starts, listing the seed URLs.
    Started {
        seeds: Vec<String>,
        /// Engine selected for the crawl: `standard`, `headless`, or `hybrid`.
        engine: String,
    },
    /// A page was fetched, parsed, and emitted as a result.
    PageFetched {
        url: String,
        status_code: i32,
        depth: i32,
        content_length: i64,
        /// Number of extracted forms (when `form_extraction` is enabled).
        forms: usize,
        /// Detected technologies (when `tech_detect` is enabled).
        technologies: Vec<String>,
    },
    /// A fetch failed after all retries.
    PageError { url: String, error: String },
    /// A discovered URL was filtered out and will not be visited.
    PageSkipped { url: String, reason: String },
    /// A same-scope link was discovered and queued for visiting.
    LinkDiscovered { url: String, from: String, depth: i32 },
    /// Live counters, emitted alongside result/skip/error events.
    Progress { results: usize, skipped: usize, failed: usize },
    /// A human-readable log line (level: `error`, `info`, `warning`, `debug`).
    Message { level: String, text: String },
    /// The session was paused via [`crate::control::CrawlControl`].
    Paused,
    /// The session was resumed.
    Resumed,
    /// Cancellation was requested; the crawl is draining and will stop soon.
    Cancelled,
    /// Emitted exactly once when the crawl ends (drained, cancelled, or expired).
    Finished { summary: SessionSummary },
}

impl CrawlerEvent {
    /// Convenience constructor for [`CrawlerEvent::Message`].
    pub fn message(level: impl Into<String>, text: impl Into<String>) -> Self {
        CrawlerEvent::Message {
            level: level.into(),
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_tag_snake_case() {
        let event = CrawlerEvent::PageFetched {
            url: "https://example.com".into(),
            status_code: 200,
            depth: 1,
            content_length: 128,
            forms: 0,
            technologies: vec![],
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "page_fetched");
        assert_eq!(json["url"], "https://example.com");
        // Fields are camelCase for JS consumers (Tauri IPC / Electron IPC).
        assert_eq!(json["statusCode"], 200);
        assert_eq!(json["contentLength"], 128);
    }

    #[test]
    fn test_event_round_trip() {
        let event = CrawlerEvent::Finished {
            summary: SessionSummary {
                results: 3,
                skipped: 1,
                failed: 0,
                visited_urls: vec!["https://example.com".into()],
                duration_ms: 250,
                cancelled: false,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("visitedUrls"), "summary fields are camelCase: {json}");
        assert!(json.contains("durationMs"), "summary fields are camelCase: {json}");
        let back: CrawlerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn test_control_events_serialize() {
        for (event, tag) in [
            (CrawlerEvent::Paused, "paused"),
            (CrawlerEvent::Resumed, "resumed"),
            (CrawlerEvent::Cancelled, "cancelled"),
        ] {
            let json = serde_json::to_value(&event).unwrap();
            assert_eq!(json["type"], tag, "control event must carry its type tag");
        }
    }
}
