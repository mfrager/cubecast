//! Shared HTTP application state and production counters.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ontology_application::ApplicationService;

/// Process-wide request counters reported by the metrics endpoint.
#[derive(Debug, Default)]
pub struct Metrics {
    requests: AtomicU64,
    errors: AtomicU64,
}

impl Metrics {
    /// Records one request and whether it failed.
    pub fn record(&self, failed: bool) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Returns `(requests, errors)`.
    #[must_use]
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        )
    }
}

struct Window {
    started: Instant,
    count: u32,
}

/// Global fixed-window request limiter.
///
/// The limit is intentionally process-wide: a single API process normally
/// fronts one RDF store, and a per-client table would add unbounded state for
/// little benefit in the default deployment.
pub struct RateLimiter {
    limit: u32,
    window: Mutex<Window>,
}

impl RateLimiter {
    /// Creates a limiter allowing `limit` requests per minute.
    #[must_use]
    pub fn new(limit: u32) -> Self {
        Self {
            limit: limit.max(1),
            window: Mutex::new(Window {
                started: Instant::now(),
                count: 0,
            }),
        }
    }

    /// Returns the configured per-minute limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Returns whether the current request fits in the window.
    pub fn allow(&self) -> bool {
        let mut window = match self.window.lock() {
            Ok(window) => window,
            Err(poisoned) => poisoned.into_inner(),
        };
        if window.started.elapsed() >= Duration::from_secs(60) {
            window.started = Instant::now();
            window.count = 0;
        }
        if window.count >= self.limit {
            false
        } else {
            window.count += 1;
            true
        }
    }
}

/// State handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// Shared application service; all interfaces call the same instance.
    pub service: Arc<ApplicationService>,
    /// When set, requests must carry `Authorization: Bearer <key>`.
    pub api_key: Option<String>,
    /// Server start time, reported by the status endpoint.
    pub started_at: DateTime<Utc>,
    /// Request counters.
    pub metrics: Arc<Metrics>,
    /// Optional global request limit.
    pub rate_limit: Option<Arc<RateLimiter>>,
}

impl AppState {
    /// Creates state with optional API-key authentication.
    #[must_use]
    pub fn new(service: Arc<ApplicationService>, api_key: Option<String>) -> Self {
        Self {
            service,
            api_key,
            started_at: Utc::now(),
            metrics: Arc::new(Metrics::default()),
            rate_limit: None,
        }
    }

    /// Creates unauthenticated state.
    #[must_use]
    pub fn open(service: Arc<ApplicationService>) -> Self {
        Self::new(service, None)
    }

    /// Enables a global per-minute request limit.
    #[must_use]
    pub fn with_rate_limit(mut self, limit: Option<u32>) -> Self {
        self.rate_limit = limit.map(|limit| Arc::new(RateLimiter::new(limit)));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_count_requests_and_errors() {
        let metrics = Metrics::default();
        assert_eq!(metrics.snapshot(), (0, 0));
        metrics.record(false);
        metrics.record(true);
        metrics.record(false);
        assert_eq!(metrics.snapshot(), (3, 1));
    }

    #[test]
    fn limiter_enforces_its_window_budget() {
        let limiter = RateLimiter::new(2);
        assert_eq!(limiter.limit(), 2);
        assert!(limiter.allow());
        assert!(limiter.allow());
        assert!(!limiter.allow());
    }

    #[test]
    fn limiter_clamps_zero_to_one() {
        let limiter = RateLimiter::new(0);
        assert_eq!(limiter.limit(), 1);
        assert!(limiter.allow());
        assert!(!limiter.allow());
    }
}
