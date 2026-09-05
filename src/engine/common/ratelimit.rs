//! Rate limiting (the reference crawler uses `projectdiscovery/jitrate`): a token-bucket
//! limiter for global/per-second & per-minute rates, plus per-host limiters.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A continuous-refill token bucket (reference crawler `jitrate.Limiter` equivalent).
#[derive(Debug)]
pub struct RateLimiter {
    interval: Duration,
    next_allowed: Mutex<Instant>,
}

impl RateLimiter {
    /// `per_second` = allowed events per second (fractional supported via
    /// per-minute variants downstream).
    pub fn new(per_second: usize) -> Self {
        let interval = if per_second == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(1.0 / per_second as f64)
        };
        RateLimiter {
            interval,
            next_allowed: Mutex::new(Instant::now()),
        }
    }

    pub fn unlimited() -> Self {
        RateLimiter::new(0)
    }

    pub fn is_unlimited(&self) -> bool {
        self.interval.is_zero()
    }

    /// Wait until a token is available, reserving the next slot.
    pub async fn wait(&self) {
        if self.is_unlimited() {
            return;
        }
        loop {
            let now = Instant::now();
            let wait_for = {
                let mut next = self.next_allowed.lock().unwrap();
                if *next <= now {
                    *next = now + self.interval;
                    None
                } else {
                    Some(*next - now)
                }
            };
            match wait_for {
                None => return,
                Some(d) => tokio::time::sleep(d).await,
            }
        }
    }
}

/// Per-host limiter registry (celestia per-host rate limiting).
#[derive(Debug, Default)]
pub struct HostRateLimiter {
    limiters: Mutex<std::collections::HashMap<String, Arc<RateLimiter>>>,
    per_second: usize,
}

impl HostRateLimiter {
    pub fn new(per_second: usize) -> Self {
        HostRateLimiter { limiters: Mutex::new(std::collections::HashMap::new()), per_second }
    }

    pub fn limiter_for(&self, host: &str) -> Arc<RateLimiter> {
        let mut map = self.limiters.lock().unwrap();
        map.entry(host.to_string())
            .or_insert_with(|| Arc::new(RateLimiter::new(self.per_second)))
            .clone()
    }
}

/// Combined pacing: global rate, per-minute rate, per-host rates, and a fixed
/// request delay (reference crawler `-rl/-rlm/-hrl/-hrlm/-rd`).
#[derive(Debug, Default)]
pub struct Pacer {
    pub global: Option<Arc<RateLimiter>>,
    pub global_minute: Option<Arc<RateLimiter>>,
    pub host: Option<HostRateLimiter>,
    pub host_minute: Option<HostRateLimiter>,
    pub delay: Duration,
}

impl Pacer {
    pub fn from_options(
        rate_limit: usize,
        rate_limit_minute: usize,
        host_rate_limit: usize,
        host_rate_limit_minute: usize,
        delay_secs: u64,
    ) -> Self {
        Pacer {
            global: (rate_limit > 0).then(|| Arc::new(RateLimiter::new(rate_limit))),
            global_minute: (rate_limit_minute > 0)
                .then(|| Arc::new(RateLimiter::new((rate_limit_minute as f64 / 60.0).ceil() as usize))),
            host: (host_rate_limit > 0).then(|| HostRateLimiter::new(host_rate_limit)),
            host_minute: (host_rate_limit_minute > 0)
                .then(|| HostRateLimiter::new((host_rate_limit_minute as f64 / 60.0).ceil() as usize)),
            delay: Duration::from_secs(delay_secs),
        }
    }

    /// Apply all pacing before a request to `host`.
    pub async fn wait(&self, host: &str) {
        if let Some(g) = &self.global {
            g.wait().await;
        }
        if let Some(gm) = &self.global_minute {
            gm.wait().await;
        }
        if let Some(h) = &self.host {
            h.limiter_for(host).wait().await;
        }
        if let Some(hm) = &self.host_minute {
            hm.limiter_for(host).wait().await;
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    #[tokio::test]
    async fn test_rate_limiter_paces() {
        let limiter = RateLimiter::new(100); // 10ms interval
        let start = Instant::now();
        for _ in 0..5 {
            limiter.wait().await;
        }
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(40), "elapsed: {elapsed:?}");
    }

    #[tokio::test]
    async fn test_unlimited_is_fast() {
        let limiter = RateLimiter::unlimited();
        let start = Instant::now();
        for _ in 0..100 {
            limiter.wait().await;
        }
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn test_host_limiters_are_per_host() {
        let hosts = HostRateLimiter::new(10);
        let a1 = hosts.limiter_for("a.com");
        let a2 = hosts.limiter_for("a.com");
        let b = hosts.limiter_for("b.com");
        assert!(Arc::ptr_eq(&a1, &a2));
        assert!(!Arc::ptr_eq(&a1, &b));
    }

    #[tokio::test]
    async fn test_pacer_delay() {
        let pacer = Pacer::from_options(0, 0, 0, 0, 0);
        // No delay configured: immediate.
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        pacer.wait("x.com").await;
        c.fetch_add(1, Ordering::SeqCst);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
