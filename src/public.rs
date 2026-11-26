//! The login-free `/public` endpoint.
//!
//! `/public` lets a client use the market-data, fundamentals, news and research
//! tools without an account: the server calls upstream with one dedicated
//! service credential configured by the operator, never with anything the
//! caller sends. Because every caller shares that credential (and its upstream
//! quota and quote connections), the endpoint
//!
//! - exposes only the `PUBLIC` allowlist in `crate::tools` — read-only market
//!   data, never account, order, watchlist or any other write/personal tool;
//! - limits tool calls per client IP per minute; and
//! - caps how many tool calls run upstream at the same time.
//!
//! The endpoint is disabled (404) unless `LONGBRIDGE_PUBLIC_TOKEN` is set.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Env var holding the service credential `/public` calls upstream with.
pub const TOKEN_ENV: &str = "LONGBRIDGE_PUBLIC_TOKEN";
/// Env var: tool calls allowed per client IP per minute.
pub const RATE_ENV: &str = "LONGBRIDGE_PUBLIC_RATE_PER_MINUTE";
/// Env var: tool calls allowed to run upstream at the same time.
pub const CONCURRENCY_ENV: &str = "LONGBRIDGE_PUBLIC_MAX_CONCURRENCY";

const DEFAULT_RATE_PER_MINUTE: u32 = 30;
const DEFAULT_MAX_CONCURRENCY: usize = 8;
const WINDOW: Duration = Duration::from_secs(60);
/// How long a call waits for a free upstream slot before it is turned away.
const QUEUE_WAIT: Duration = Duration::from_secs(10);
/// Drop idle per-IP windows once the table grows past this many entries.
const PRUNE_THRESHOLD: usize = 10_000;

/// Configuration and shared state of the `/public` endpoint.
pub struct PublicAccess {
    token: String,
    rate_per_minute: u32,
    max_concurrency: usize,
    windows: Mutex<HashMap<String, (Instant, u32)>>,
    slots: Arc<Semaphore>,
    queue_wait: Duration,
}

impl std::fmt::Debug for PublicAccess {
    // Never print the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicAccess")
            .field("rate_per_minute", &self.rate_per_minute)
            .field("max_concurrency", &self.max_concurrency)
            .finish_non_exhaustive()
    }
}

/// Why a `/public` tool call was turned away.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The client IP used up its calls for the current minute.
    RateLimited { retry_after_secs: u64 },
    /// Every upstream slot stayed busy for [`QUEUE_WAIT`].
    Busy,
}

impl PublicAccess {
    pub fn new(token: String, rate_per_minute: u32, max_concurrency: usize) -> Self {
        let max_concurrency = max_concurrency.max(1);
        Self {
            token,
            rate_per_minute: rate_per_minute.max(1),
            max_concurrency,
            windows: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(max_concurrency)),
            queue_wait: QUEUE_WAIT,
        }
    }

    /// Read the configuration from the environment. `None` (endpoint disabled)
    /// when [`TOKEN_ENV`] is unset or blank.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var(TOKEN_ENV).ok()?.trim().to_string();
        if token.is_empty() {
            return None;
        }
        let rate = parse_env(RATE_ENV).unwrap_or(DEFAULT_RATE_PER_MINUTE);
        let concurrency = parse_env(CONCURRENCY_ENV).unwrap_or(DEFAULT_MAX_CONCURRENCY);
        Some(Self::new(token, rate, concurrency))
    }

    /// The service credential sent upstream for every `/public` call.
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn rate_per_minute(&self) -> u32 {
        self.rate_per_minute
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Count one tool call against `client`'s per-minute budget.
    pub fn check_rate(&self, client: &str) -> Result<(), Refusal> {
        self.check_rate_at(client, Instant::now())
    }

    fn check_rate_at(&self, client: &str, now: Instant) -> Result<(), Refusal> {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        if windows.len() > PRUNE_THRESHOLD {
            windows.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        let entry = windows.entry(client.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        if entry.1 >= self.rate_per_minute {
            let elapsed = now.duration_since(entry.0);
            let retry_after_secs = WINDOW.saturating_sub(elapsed).as_secs().max(1);
            return Err(Refusal::RateLimited { retry_after_secs });
        }
        entry.1 += 1;
        Ok(())
    }

    /// Wait (up to [`QUEUE_WAIT`] by default) for one of the upstream slots. The slot is
    /// released when the returned permit is dropped.
    pub async fn acquire_slot(&self) -> Result<OwnedSemaphorePermit, Refusal> {
        match tokio::time::timeout(self.queue_wait, self.slots.clone().acquire_owned()).await {
            Ok(Ok(permit)) => Ok(permit),
            _ => Err(Refusal::Busy),
        }
    }
}

fn parse_env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.trim().parse().ok()
}

/// The client address used as the rate-limit key: the first `X-Forwarded-For`
/// hop (set by the ingress in front of this service), then `X-Real-IP`, else
/// `"unknown"` — which puts every caller without either header in one shared
/// bucket rather than exempting them.
pub fn client_key(headers: &HeaderMap) -> String {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let real_ip = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    forwarded.or(real_ip).unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_counts_per_client_and_resets_after_a_minute() {
        let access = PublicAccess::new("t".into(), 2, 1);
        let start = Instant::now();
        assert_eq!(access.check_rate_at("a", start), Ok(()));
        assert_eq!(access.check_rate_at("a", start), Ok(()));
        assert!(matches!(
            access.check_rate_at("a", start + Duration::from_secs(20)),
            Err(Refusal::RateLimited {
                retry_after_secs: 40
            })
        ));
        // Another client has its own budget.
        assert_eq!(access.check_rate_at("b", start), Ok(()));
        // A new window starts a minute after the first call.
        assert_eq!(access.check_rate_at("a", start + WINDOW), Ok(()));
    }

    #[tokio::test]
    async fn busy_slots_turn_calls_away_after_the_queue_wait() {
        let mut access = PublicAccess::new("t".into(), 10, 1);
        access.queue_wait = Duration::from_millis(20);
        let held = access.acquire_slot().await.expect("first slot");
        assert_eq!(access.acquire_slot().await.unwrap_err(), Refusal::Busy);
        drop(held);
        assert!(access.acquire_slot().await.is_ok());
    }

    #[test]
    fn client_key_prefers_first_forwarded_hop() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_key(&headers), "unknown");
        headers.insert("x-real-ip", "198.51.100.7".parse().unwrap());
        assert_eq!(client_key(&headers), "198.51.100.7");
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(client_key(&headers), "203.0.113.9");
    }

    #[test]
    fn debug_output_hides_the_token() {
        let access = PublicAccess::new("secret-token".into(), 1, 1);
        assert!(!format!("{access:?}").contains("secret-token"));
    }
}
