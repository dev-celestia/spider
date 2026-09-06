//! Cooperative crawl control for UI hosts: external cancellation and pause/resume.
//!
//! `CrawlControl` replaces the bare `Arc<AtomicBool>` cancel token that previously lived
//! only inside [`crate::runner::Runner`]. UI apps hold the handle side (`cancel()`,
//! `pause()`, `resume()`) while the engine polls the shared flags, so a Tauri command or
//! any other UI event handler can drive the crawl without owning the task.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Shared control flags for a running crawl.
#[derive(Default)]
pub struct CrawlControl {
    cancel: AtomicBool,
    paused: AtomicBool,
}

impl CrawlControl {
    /// Wrap an existing cancellation flag (back-compat with `Arc<AtomicBool>` tokens).
    pub fn from_cancel(cancel: Arc<AtomicBool>) -> Self {
        CrawlControl {
            cancel: AtomicBool::new(cancel.load(Ordering::SeqCst)),
            paused: AtomicBool::new(false),
        }
    }

    /// Request cooperative cancellation. Workers drain their current item and stop.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Pause the crawl: workers idle (async, no CPU spin) until resumed or cancelled.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume a paused crawl. Resuming a non-paused crawl is a no-op.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Async-friendly pause gate: awaits (with a 100ms poll interval) while paused,
    /// returning immediately when cancelled so workers also observe the stop.
    pub async fn wait_if_paused(&self) {
        while self.paused.load(Ordering::SeqCst) && !self.cancel.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Shared handle for passing into engines and background tasks.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancel_sets_flags() {
        let control = CrawlControl::default();
        assert!(!control.is_cancelled());
        control.cancel();
        assert!(control.is_cancelled());
    }

    #[test]
    fn test_pause_resume_lifecycle() {
        let control = CrawlControl::default();
        control.pause();
        assert!(control.is_paused());
        control.resume();
        assert!(!control.is_paused());
    }

    #[test]
    fn test_cancel_unblocks_pause() {
        let control = CrawlControl::default();
        control.pause();
        control.cancel();
        // Cancel must clear the paused flag so workers do not idle forever.
        assert!(!control.is_paused());
    }

    #[test]
    fn test_from_cancel_preserves_state() {
        let flag = Arc::new(AtomicBool::new(true));
        let control = CrawlControl::from_cancel(flag);
        assert!(control.is_cancelled());
    }
}
