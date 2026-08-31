//! Generic per-identity connection-context pool.
//!
//! Both `QuoteContext` and `TradeContext` open a persistent WebSocket
//! connection to Longbridge. Creating one per tool call would exhaust the
//! server-side per-account connection limit under concurrent load. This
//! module caches one context per authenticated identity so all tool calls
//! from the same user in this process share a single connection, regardless
//! of which context type is being pooled.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub struct PoolSettings {
    pub max_entries: usize,
    pub idle_ttl: Duration,
}

impl PoolSettings {
    pub fn from_env(
        idle_ttl_env: &str,
        max_entries_env: &str,
        default_max_entries: usize,
        default_idle_ttl_secs: u64,
    ) -> Self {
        let max_entries = std::env::var(max_entries_env)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(default_max_entries);
        let idle_ttl = std::env::var(idle_ttl_env)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(default_idle_ttl_secs));

        Self {
            max_entries,
            idle_ttl,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PoolKey {
    identity: String,
}

impl PoolKey {
    fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
        }
    }
}

struct PoolEntry<T> {
    value: T,
    token_fingerprint: String,
    last_used: Instant,
}

/// Combined state protected by a single Mutex so `last_prune` is always
/// consistent with `entries` without a second lock acquisition.
struct PoolInner<T> {
    entries: HashMap<PoolKey, PoolEntry<T>>,
    /// Timestamp of the last time `prune_idle` ran inline (on the hot path).
    /// Used to gate the inline scan so it only runs every `idle_ttl / 4`,
    /// keeping hot-path Mutex hold short. The background sweeper handles the
    /// rest.
    last_prune: Instant,
}

/// Metric hooks so this generic engine can report into whichever
/// pool-specific Prometheus series the caller owns, without depending on
/// `crate::metrics` directly.
pub struct PoolMetrics {
    pub record_event: fn(&str, u64),
    pub set_entries: fn(usize),
}

pub struct Pool<T>
where
    T: Clone,
{
    settings: PoolSettings,
    metrics: PoolMetrics,
    inner: Mutex<PoolInner<T>>,
}

impl<T> Pool<T>
where
    T: Clone,
{
    pub fn new(settings: PoolSettings, metrics: PoolMetrics) -> Self {
        Self {
            settings,
            metrics,
            inner: Mutex::new(PoolInner {
                entries: HashMap::new(),
                last_prune: Instant::now(),
            }),
        }
    }

    pub fn get_or_insert_with(
        &self,
        key: PoolKey,
        token_fingerprint: String,
        init: impl FnOnce() -> T,
    ) -> T {
        self.get_or_insert_with_at(Instant::now(), key, token_fingerprint, init)
    }

    fn get_or_insert_with_at(
        &self,
        now: Instant,
        key: PoolKey,
        token_fingerprint: String,
        init: impl FnOnce() -> T,
    ) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());

        // Inline idle prune — time-gated to every idle_ttl/4 so the hot path
        // does not run an O(N) HashMap::retain on every single cache hit.
        // The background sweeper handles cleanup between prune windows.
        let prune_interval = self.settings.idle_ttl / 4;
        if now.saturating_duration_since(inner.last_prune) >= prune_interval {
            let removed = Self::do_prune_idle(&mut inner.entries, self.settings.idle_ttl, now);
            if removed > 0 {
                (self.metrics.record_event)("evict_idle", removed as u64);
            }
            inner.last_prune = now;
        }

        if let Some(value) = inner.entries.get_mut(&key).and_then(|entry| {
            if entry.token_fingerprint == token_fingerprint {
                entry.last_used = now;
                Some(entry.value.clone())
            } else {
                None
            }
        }) {
            (self.metrics.record_event)("hit", 1);
            (self.metrics.set_entries)(inner.entries.len());
            return value;
        }

        if inner.entries.contains_key(&key) {
            inner.entries.remove(&key);
            (self.metrics.record_event)("evict_rotated_token", 1);
        }

        if inner.entries.len() >= self.settings.max_entries
            && Self::do_evict_lru(&mut inner.entries).is_some()
        {
            (self.metrics.record_event)("evict_capacity", 1);
        }

        (self.metrics.record_event)("miss", 1);
        // `init()` is called while the Mutex is held. `QuoteContext::new`/
        // `TradeContext::new` spawn their WS task on the SDK's own Tokio
        // runtime and return immediately, so this does not block the calling
        // thread. The trade-off is that concurrent first-use requests for the
        // SAME key are serialized here — preventing duplicate WS connections
        // for the same user, which is the desired behavior.
        let value = init();
        inner.entries.insert(
            key,
            PoolEntry {
                value: value.clone(),
                token_fingerprint,
                last_used: now,
            },
        );
        (self.metrics.set_entries)(inner.entries.len());
        value
    }

    pub fn remove_identity(&self, key: &PoolKey) {
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let before = inner.entries.len();
        inner.entries.remove(key);
        let removed = before.saturating_sub(inner.entries.len());
        if removed > 0 {
            (self.metrics.record_event)("evict_explicit", removed as u64);
            (self.metrics.set_entries)(inner.entries.len());
        }
    }

    pub fn prune_idle_now(&self) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let removed = Self::do_prune_idle(&mut inner.entries, self.settings.idle_ttl, now);
        if removed > 0 {
            (self.metrics.record_event)("evict_idle", removed as u64);
            (self.metrics.set_entries)(inner.entries.len());
        }
    }

    fn do_prune_idle(
        entries: &mut HashMap<PoolKey, PoolEntry<T>>,
        idle_ttl: Duration,
        now: Instant,
    ) -> usize {
        let before = entries.len();
        entries.retain(|_, entry| now.saturating_duration_since(entry.last_used) <= idle_ttl);
        before.saturating_sub(entries.len())
    }

    fn do_evict_lru(entries: &mut HashMap<PoolKey, PoolEntry<T>>) -> Option<PoolKey> {
        let key = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone())?;
        entries.remove(&key);
        Some(key)
    }

    pub fn idle_ttl(&self) -> Duration {
        self.settings.idle_ttl
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .entries
            .len()
    }

    #[cfg(test)]
    fn contains_key(&self, key: &PoolKey) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .entries
            .contains_key(key)
    }
}

/// Start a background sweeper for `pool`, gated so it only ever spawns once
/// per `started` flag. Call this on every pool access — the `OnceLock` makes
/// repeat calls free.
pub fn ensure_sweeper_started<T: Clone + Send + Sync + 'static>(
    pool: &'static Pool<T>,
    started: &'static OnceLock<()>,
) {
    started.get_or_init(|| {
        // Guard: tokio::spawn requires an active Tokio runtime. Skip silently
        // when called from a non-async context (e.g. a sync unit test or a
        // CLI path without a runtime). The time-gated inline prune in
        // get_or_insert_with_at handles cleanup in the absence of the sweeper.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let interval = sweep_interval(pool.idle_ttl());
            handle.spawn(async move {
                let mut ticker = tokio::time::interval(interval);
                loop {
                    ticker.tick().await;
                    pool.prune_idle_now();
                }
            });
        }
    });
}

fn sweep_interval(idle_ttl: Duration) -> Duration {
    if idle_ttl < Duration::from_secs(1) {
        Duration::from_secs(1)
    } else if idle_ttl < Duration::from_secs(60) {
        idle_ttl
    } else {
        Duration::from_secs(60)
    }
}

pub fn cache_key_for_token(token: &str) -> PoolKey {
    let identity = jwt_identity(token)
        .map(|identity| format!("jwt:{}", sha256_hex(identity.as_bytes())))
        .unwrap_or_else(|| format!("token:{}", token_fingerprint(token)));
    PoolKey::new(identity)
}

pub fn token_fingerprint(token: &str) -> String {
    sha256_hex(token.as_bytes())
}

fn jwt_subject(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let mut padded = payload.to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let bytes = URL_SAFE.decode(padded).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims["sub"]
        .as_str()
        .filter(|sub| !sub.is_empty())
        .map(str::to_owned)
}

fn jwt_identity(token: &str) -> Option<String> {
    let subject = jwt_subject(token)?;
    stable_identity_from_subject(&subject)
}

fn stable_identity_from_subject(subject: &str) -> Option<String> {
    match serde_json::from_str::<serde_json::Value>(subject) {
        Ok(serde_json::Value::Object(map)) => {
            const ID_FIELDS: &[&str] = &[
                "user_id",
                "member_id",
                "account_id",
                "account_no",
                "account",
                "uid",
                "id",
                "open_id",
            ];
            let channel = stable_json_field(&map, "account_channel").unwrap_or_default();
            for field in ID_FIELDS {
                if let Some(value) = stable_json_field(&map, field) {
                    return Some(format!("account_channel={channel};{field}={value}"));
                }
            }
            None
        }
        Ok(_) => None,
        Err(_) => Some(subject.to_owned()),
    }
}

fn stable_json_field(
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    match map.get(key)? {
        serde_json::Value::String(value) if !value.is_empty() => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    fn noop_metrics() -> PoolMetrics {
        PoolMetrics {
            record_event: |_, _| {},
            set_entries: |_| {},
        }
    }

    fn settings(max_entries: usize, idle_ttl: Duration) -> PoolSettings {
        PoolSettings {
            max_entries,
            idle_ttl,
        }
    }

    fn key(name: &str) -> PoolKey {
        PoolKey::new(format!("identity:{name}"))
    }

    fn jwt_with_sub(sub: &str, signature: &str) -> String {
        let header = base64::Engine::encode(&URL_SAFE_NO_PAD, r#"{"alg":"none"}"#);
        let claims = serde_json::json!({ "sub": sub });
        let payload = base64::Engine::encode(&URL_SAFE_NO_PAD, claims.to_string());
        format!("{header}.{payload}.{signature}")
    }

    #[test]
    fn concurrent_first_use_initializes_once() {
        let pool = Arc::new(Pool::new(
            settings(16, Duration::from_secs(60)),
            noop_metrics(),
        ));
        let init_count = Arc::new(AtomicUsize::new(0));
        let key = key("same-user");
        let token = token_fingerprint("access-token");

        let mut handles = Vec::new();
        for _ in 0..32 {
            let pool = pool.clone();
            let init_count = init_count.clone();
            let key = key.clone();
            let token = token.clone();
            handles.push(std::thread::spawn(move || {
                pool.get_or_insert_with_at(Instant::now(), key, token, || {
                    init_count.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(10));
                    42usize
                })
            }));
        }

        for handle in handles {
            assert_eq!(handle.join().unwrap(), 42);
        }
        assert_eq!(init_count.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn idle_entries_are_evicted_after_ttl() {
        let pool = Pool::new(settings(16, Duration::from_secs(5)), noop_metrics());
        let start = Instant::now();
        let key = key("idle-user");
        let token = token_fingerprint("access-token");

        assert_eq!(
            pool.get_or_insert_with_at(start, key.clone(), token.clone(), || 1usize),
            1
        );
        assert_eq!(
            pool.get_or_insert_with_at(
                start + Duration::from_secs(4),
                key.clone(),
                token.clone(),
                || 2usize
            ),
            1
        );
        assert_eq!(
            pool.get_or_insert_with_at(start + Duration::from_secs(10), key, token, || 3usize),
            3
        );
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn capacity_eviction_removes_least_recently_used_entry() {
        let pool = Pool::new(settings(2, Duration::from_secs(60)), noop_metrics());
        let start = Instant::now();
        let a = key("a");
        let b = key("b");
        let c = key("c");

        pool.get_or_insert_with_at(start, a.clone(), token_fingerprint("a"), || "a");
        pool.get_or_insert_with_at(
            start + Duration::from_secs(1),
            b.clone(),
            token_fingerprint("b"),
            || "b",
        );
        pool.get_or_insert_with_at(
            start + Duration::from_secs(2),
            a.clone(),
            token_fingerprint("a"),
            || "a",
        );
        pool.get_or_insert_with_at(
            start + Duration::from_secs(3),
            c.clone(),
            token_fingerprint("c"),
            || "c",
        );

        assert_eq!(pool.len(), 2);
        assert!(
            pool.contains_key(&a),
            "entry 'a' (most recently used) should survive LRU eviction"
        );
        assert!(
            !pool.contains_key(&b),
            "entry 'b' (least recently used) should be LRU-evicted"
        );
        assert!(
            pool.contains_key(&c),
            "entry 'c' (just inserted) should be retained"
        );
    }

    #[test]
    fn jwt_subject_key_survives_token_refresh_without_storing_plaintext() {
        let sub = r#"{"account_channel":"lb","user_id":"u-1"}"#;
        let token_a = jwt_with_sub(sub, "signature-a");
        let token_b = jwt_with_sub(sub, "signature-b");

        let key_a = cache_key_for_token(&token_a);
        let key_b = cache_key_for_token(&token_b);

        assert_eq!(
            key_a, key_b,
            "same JWT subject should produce the same cache key regardless of signature"
        );
        assert!(
            !key_a.identity.contains(&token_a),
            "cache key must not contain the raw token bytes"
        );
        assert!(
            !key_a.identity.contains(sub),
            "cache key must not contain the raw JWT subject"
        );
    }

    #[test]
    fn jwt_subject_without_user_identity_falls_back_to_token_key() {
        let sub = r#"{"account_channel":"lb"}"#;
        let token_a = jwt_with_sub(sub, "signature-a");
        let token_b = jwt_with_sub(sub, "signature-b");

        let key_a = cache_key_for_token(&token_a);
        let key_b = cache_key_for_token(&token_b);

        assert_ne!(key_a, key_b);
    }

    #[test]
    fn token_rotation_for_same_identity_replaces_cached_context() {
        let pool = Pool::new(settings(16, Duration::from_secs(60)), noop_metrics());
        let start = Instant::now();
        let key = key("rotating-user");

        assert_eq!(
            pool.get_or_insert_with_at(start, key.clone(), token_fingerprint("old-token"), || {
                1usize
            }),
            1
        );
        assert_eq!(
            pool.get_or_insert_with_at(
                start + Duration::from_secs(1),
                key.clone(),
                token_fingerprint("new-token"),
                || 2usize
            ),
            2
        );

        assert_eq!(pool.len(), 1);
        assert!(
            pool.contains_key(&key),
            "rotating user's key should remain in pool after token swap"
        );
    }
}
