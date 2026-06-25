//! WebSocket QuoteContext pool.
//!
//! `QuoteContext` opens a persistent WebSocket connection to Longbridge.
//! Creating one per tool call would exhaust the server-side per-account
//! connection limit under concurrent load.  This module caches one
//! `QuoteContext` per authenticated identity so all quote tool calls from the
//! same user in this process share a single connection.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use longbridge::quote::QuoteContext;
use sha2::{Digest, Sha256};

const DEFAULT_IDLE_TTL_SECS: u64 = 10 * 60;
const DEFAULT_MAX_ENTRIES: usize = 1024;
const IDLE_TTL_ENV: &str = "LONGBRIDGE_MCP_QUOTE_WS_IDLE_TTL_SECS";
const MAX_ENTRIES_ENV: &str = "LONGBRIDGE_MCP_QUOTE_WS_MAX_CONTEXTS";

static POOL: LazyLock<Pool<QuoteContext>> = LazyLock::new(|| Pool::new(PoolSettings::from_env()));
/// Set to `true` only after the sweeper task is successfully spawned.
/// Unlike a `OnceLock`, this allows retrying on the next call if the first
/// call arrived before the Tokio runtime was available.
static SWEEPER_STARTED: AtomicBool = AtomicBool::new(false);
/// Monotonically increasing generation counter; incremented on every insert.
/// Callers receive the generation of the entry they obtained so they can do
/// generation-guarded eviction (see `evict_generation`).
static NEXT_GEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
struct PoolSettings {
    max_entries: usize,
    idle_ttl: Duration,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_MAX_ENTRIES,
            idle_ttl: Duration::from_secs(DEFAULT_IDLE_TTL_SECS),
        }
    }
}

impl PoolSettings {
    fn from_env() -> Self {
        let defaults = Self::default();
        let max_entries = std::env::var(MAX_ENTRIES_ENV)
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(defaults.max_entries);
        let idle_ttl = std::env::var(IDLE_TTL_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .map(Duration::from_secs)
            .unwrap_or(defaults.idle_ttl);

        Self {
            max_entries,
            idle_ttl,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PoolKey {
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
    /// Unique, monotonically increasing identifier for this specific connection.
    /// Used by `evict_generation` to ensure a concurrent error handler does not
    /// remove a healthy replacement connection created after the broken one was
    /// evicted.
    generation: u64,
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

struct Pool<T>
where
    T: Clone,
{
    settings: PoolSettings,
    inner: Mutex<PoolInner<T>>,
}

impl<T> Pool<T>
where
    T: Clone,
{
    fn new(settings: PoolSettings) -> Self {
        Self {
            settings,
            inner: Mutex::new(PoolInner {
                entries: HashMap::new(),
                last_prune: Instant::now(),
            }),
        }
    }

    /// Returns `(value, generation)`.  The generation uniquely identifies this
    /// specific connection instance; pass it to `evict_generation` on error to
    /// avoid racing with a concurrent reconnect.
    fn get_or_insert_with(
        &self,
        key: PoolKey,
        token_fingerprint: String,
        init: impl FnOnce() -> T,
    ) -> (T, u64) {
        self.get_or_insert_with_gen_at(Instant::now(), key, token_fingerprint, init)
    }

    /// Test-facing wrapper that discards the generation so existing unit tests
    /// do not need to change.
    #[cfg(test)]
    fn get_or_insert_with_at(
        &self,
        now: Instant,
        key: PoolKey,
        token_fingerprint: String,
        init: impl FnOnce() -> T,
    ) -> T {
        self.get_or_insert_with_gen_at(now, key, token_fingerprint, init)
            .0
    }

    fn get_or_insert_with_gen_at(
        &self,
        now: Instant,
        key: PoolKey,
        token_fingerprint: String,
        init: impl FnOnce() -> T,
    ) -> (T, u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());

        // Inline idle prune — time-gated to every idle_ttl/4 so the hot path
        // does not run an O(N) HashMap::retain on every single cache hit.
        // The background sweeper handles cleanup between prune windows.
        let prune_interval = self.settings.idle_ttl / 4;
        if now.saturating_duration_since(inner.last_prune) >= prune_interval {
            let removed = Self::do_prune_idle(&mut inner.entries, self.settings.idle_ttl, now);
            if removed > 0 {
                crate::metrics::record_quote_ws_pool_event("evict_idle", removed as u64);
            }
            inner.last_prune = now;
        }

        if let Some((value, entry_gen)) = inner.entries.get_mut(&key).and_then(|entry| {
            if entry.token_fingerprint == token_fingerprint {
                entry.last_used = now;
                Some((entry.value.clone(), entry.generation))
            } else {
                None
            }
        }) {
            crate::metrics::record_quote_ws_pool_event("hit", 1);
            crate::metrics::set_quote_ws_pool_entries(inner.entries.len());
            return (value, entry_gen);
        }

        // #7: use remove().is_some() instead of contains_key + remove (two lookups → one)
        if inner.entries.remove(&key).is_some() {
            crate::metrics::record_quote_ws_pool_event("evict_rotated_token", 1);
        }

        if inner.entries.len() >= self.settings.max_entries
            && Self::do_evict_lru(&mut inner.entries).is_some()
        {
            crate::metrics::record_quote_ws_pool_event("evict_capacity", 1);
        }

        crate::metrics::record_quote_ws_pool_event("miss", 1);
        // `init()` is called while the Mutex is held. `QuoteContext::new` spawns
        // its WS task on the SDK's own Tokio runtime and returns immediately, so
        // this does not block the calling thread. The trade-off is that concurrent
        // first-use requests for the SAME key are serialized here — preventing
        // duplicate WS connections for the same user, which is the desired behavior.
        let value = init();
        let generation = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
        inner.entries.insert(
            key,
            PoolEntry {
                value: value.clone(),
                token_fingerprint,
                last_used: now,
                generation,
            },
        );
        crate::metrics::set_quote_ws_pool_entries(inner.entries.len());
        (value, generation)
    }

    fn prune_idle_now(&self) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|err| err.into_inner());
        let removed = Self::do_prune_idle(&mut inner.entries, self.settings.idle_ttl, now);
        if removed > 0 {
            crate::metrics::record_quote_ws_pool_event("evict_idle", removed as u64);
            crate::metrics::set_quote_ws_pool_entries(inner.entries.len());
        }
    }

    fn do_prune_idle(
        entries: &mut HashMap<PoolKey, PoolEntry<T>>,
        idle_ttl: Duration,
        now: Instant,
    ) -> usize {
        let before = entries.len();
        // Use `<` so entries are evicted once they reach idle_ttl, not kept for
        // one extra prune window when age == idle_ttl exactly.
        entries.retain(|_, entry| now.saturating_duration_since(entry.last_used) < idle_ttl);
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

/// Return the cached `QuoteContext` for `token` and its generation.
///
/// `make_config` is a lazy factory: it is only called on a cache miss so
/// callers avoid building a `Config` (and its `Arc` allocation) on every
/// cache hit. Pass `|| mctx.create_config()` rather than
/// `mctx.create_config()`.
///
/// The returned `generation` uniquely identifies this specific connection
/// instance. Pass it to `evict_generation` on error to avoid the cascading
/// eviction race where a concurrent error handler removes a healthy replacement
/// connection.
pub async fn get_or_init_quote(
    token: &str,
    make_config: impl FnOnce() -> Arc<longbridge::Config>,
) -> (QuoteContext, u64) {
    ensure_idle_sweeper_started();

    let key = cache_key_for_token(token);
    let token_fingerprint = token_fingerprint(token);
    POOL.get_or_insert_with(key, token_fingerprint, || {
        let (ctx, _) = QuoteContext::new(make_config());
        ctx
    })
}

/// Evict the cached `QuoteContext` only if it still has the given `generation`.
///
/// This prevents the cascading eviction race: if Thread A errors, evicts its
/// broken connection, and immediately creates a healthy replacement, Thread B's
/// subsequent error call will only evict the replacement if it carries the same
/// generation — which it won't, because the replacement was assigned a new
/// generation on insert. Thread B's eviction is therefore a no-op, and the
/// healthy connection survives.
pub fn evict_generation(token: &str, generation: u64) {
    let key = cache_key_for_token(token);
    let mut inner = POOL.inner.lock().unwrap_or_else(|err| err.into_inner());
    if inner.entries.get(&key).map(|e| e.generation) == Some(generation) {
        inner.entries.remove(&key);
        crate::metrics::record_quote_ws_pool_event("evict_explicit", 1);
        crate::metrics::set_quote_ws_pool_entries(inner.entries.len());
    }
}

fn ensure_idle_sweeper_started() {
    // Use an AtomicBool instead of OnceLock so we can retry on the next call
    // if the first call arrived before the Tokio runtime was available.
    // OnceLock would permanently record "started" even when spawn was skipped,
    // preventing any future attempt to start the sweeper.
    if SWEEPER_STARTED.load(Ordering::Acquire) {
        return;
    }
    // Only spawn (and record) if a Tokio runtime is currently active.
    // If not, the time-gated inline prune handles cleanup; we retry on the next call.
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    // Use compare-and-swap so only one thread wins the race to spawn.
    if SWEEPER_STARTED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let interval = sweep_interval(POOL.settings.idle_ttl);
        handle.spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                POOL.prune_idle_now();
            }
        });
    }
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

fn cache_key_for_token(token: &str) -> PoolKey {
    let identity = jwt_identity(token)
        .map(|identity| format!("jwt:{}", sha256_hex(identity.as_bytes())))
        .unwrap_or_else(|| format!("token:{}", token_fingerprint(token)));
    PoolKey::new(identity)
}

fn token_fingerprint(token: &str) -> String {
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

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
        let pool = Arc::new(Pool::new(settings(16, Duration::from_secs(60))));
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
        let pool = Pool::new(settings(16, Duration::from_secs(5)));
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
        let pool = Pool::new(settings(2, Duration::from_secs(60)));
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
        let pool = Pool::new(settings(16, Duration::from_secs(60)));
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

    /// Stress test: N users × M concurrent requests each.
    ///
    /// Verifies:
    ///   1. Pool size never exceeds max_entries under concurrent insertion.
    ///   2. Each user's init closure runs exactly once regardless of concurrency.
    ///   3. Hit rate is ≥ 90% when users are stable (no token rotation).
    ///
    /// Run with:
    ///   cargo test stress_pool_bounded_concurrency -- --nocapture --ignored
    #[test]
    #[ignore]
    fn stress_pool_bounded_concurrency() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        const USERS: usize = 200;
        const REQUESTS_PER_USER: usize = 50;
        const MAX_POOL: usize = 64; // force LRU eviction (200 users > 64 slots)

        let pool = Arc::new(Pool::new(settings(MAX_POOL, Duration::from_secs(300))));
        let total_inits = Arc::new(AtomicUsize::new(0));
        let total_hits = Arc::new(AtomicUsize::new(0));
        let total_requests = USERS * REQUESTS_PER_USER;

        let start = Instant::now();
        let mut handles = Vec::with_capacity(USERS * REQUESTS_PER_USER);

        for user in 0..USERS {
            for _ in 0..REQUESTS_PER_USER {
                let pool = pool.clone();
                let total_inits = total_inits.clone();
                let total_hits = total_hits.clone();
                handles.push(std::thread::spawn(move || {
                    let k = key(&format!("user-{user}"));
                    let fp = token_fingerprint(&format!("token-{user}"));
                    let was_hit = {
                        let before = pool.len();
                        pool.get_or_insert_with(k, fp, || {
                            total_inits.fetch_add(1, Ordering::Relaxed);
                            user // value = user id
                        });
                        let after = pool.len();
                        // "hit" if pool didn't grow (entry already existed)
                        after <= before
                    };
                    if was_hit {
                        total_hits.fetch_add(1, Ordering::Relaxed);
                    }
                }));
            }
        }

        for h in handles {
            h.join().unwrap();
        }

        let elapsed = start.elapsed();
        let inits = total_inits.load(Ordering::Relaxed);
        let hits = total_hits.load(Ordering::Relaxed);
        let pool_size = pool.len();
        let hit_rate = hits as f64 / total_requests as f64 * 100.0;
        let throughput = total_requests as f64 / elapsed.as_secs_f64();

        eprintln!("\n── ws_pool stress ─────────────────────────────");
        eprintln!("  users:          {USERS}");
        eprintln!("  requests:       {total_requests} ({REQUESTS_PER_USER}/user)");
        eprintln!("  threads:        {total_requests}");
        eprintln!("  max_pool:       {MAX_POOL}");
        eprintln!("  pool size:      {pool_size}  (≤ {MAX_POOL} ✓)");
        eprintln!("  total inits:    {inits}  (≤ {USERS} expected)");
        eprintln!("  hit rate:       {hit_rate:.1}%");
        eprintln!("  throughput:     {throughput:.0} ops/s");
        eprintln!("  elapsed:        {:.2?}", elapsed);
        eprintln!("────────────────────────────────────────────────");

        assert!(
            pool_size <= MAX_POOL,
            "pool size {pool_size} exceeded max {MAX_POOL}"
        );
        assert!(
            inits <= USERS,
            "total inits {inits} exceeded user count {USERS} — double-init detected"
        );
        assert!(
            hit_rate >= 50.0,
            "hit rate {hit_rate:.1}% too low — pool may not be caching effectively"
        );
    }

    /// High-concurrency stress suite. Four scenarios in one run:
    ///
    ///   A) Hot pool, all-hit  — 1000 users, warm cache, max_entries=1024.
    ///      Measures pure Mutex+clone throughput with no evictions.
    ///   B) Capacity pressure  — 500 users, max_entries=64.
    ///      Constant LRU evictions; verifies pool never exceeds cap.
    ///   C) Token-rotation churn — 200 users × 5 token rotations each.
    ///      Every rotation evicts the old entry; measures rotated-token metric.
    ///   D) Mixed init + hit    — 100 users × 200 req, init simulates
    ///      10 ms blocking work; measures that the global Mutex serializes
    ///      inits (only one init per user) even under contention.
    ///
    /// Run with:
    ///   cargo test stress_high_concurrency -- --nocapture --ignored
    #[test]
    #[ignore]
    fn stress_high_concurrency() {
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        use std::time::Instant;

        fn run_scenario(
            label: &str,
            pool: Arc<Pool<usize>>,
            users: usize,
            req_per_user: usize,
            token_rotations: usize, // how many distinct token fingerprints per user
            init_delay_ms: u64,
        ) -> (f64, f64, usize, usize) {
            let total_inits = Arc::new(AtomicUsize::new(0));
            let total_requests = users * req_per_user;
            let start = Instant::now();
            let mut handles = Vec::with_capacity(total_requests);

            for user in 0..users {
                for req in 0..req_per_user {
                    let pool = pool.clone();
                    let total_inits = total_inits.clone();
                    let rotation = if token_rotations > 1 {
                        req / (req_per_user / token_rotations).max(1)
                    } else {
                        0
                    };
                    handles.push(std::thread::spawn(move || {
                        let k = key(&format!("user-{user}"));
                        let fp = token_fingerprint(&format!("token-{user}-rot{rotation}"));
                        pool.get_or_insert_with(k, fp, || {
                            total_inits.fetch_add(1, Relaxed);
                            if init_delay_ms > 0 {
                                std::thread::sleep(std::time::Duration::from_millis(init_delay_ms));
                            }
                            user
                        })
                    }));
                }
            }

            for h in handles {
                h.join().unwrap();
            }

            let elapsed = start.elapsed();
            let inits = total_inits.load(Relaxed);
            let pool_size = pool.len();
            let throughput = total_requests as f64 / elapsed.as_secs_f64();
            eprintln!(
                "  {label:<30} {:>6} req | pool={:>4} | inits={:>5} | {throughput:>9.0} ops/s | {:>6.2?}",
                total_requests, pool_size, inits, elapsed
            );
            (throughput, elapsed.as_secs_f64(), pool_size, inits)
        }

        eprintln!("\n══ ws_pool high-concurrency stress ══════════════════════════════");
        eprintln!(
            "  {:<30} {:>6}     {:>5}   {:>5}   {:>9}   {:>6}",
            "scenario", "reqs", "pool", "inits", "ops/s", "time"
        );
        eprintln!("  {}", "─".repeat(72));

        // A: Hot pool, all cache hits
        let (tp_a, _, pool_a, inits_a) = run_scenario(
            "A: hot pool (1000u × 50r, cap=1024)",
            Arc::new(Pool::new(settings(
                1024,
                std::time::Duration::from_secs(300),
            ))),
            1000,
            50,
            1,
            0,
        );
        assert!(pool_a <= 1024, "A: pool {pool_a} > cap 1024");
        assert!(
            inits_a <= 1000,
            "A: inits {inits_a} > users 1000 — double-init detected"
        );

        // B: Capacity pressure — constant LRU eviction
        let (tp_b, _, pool_b, _) = run_scenario(
            "B: LRU pressure (500u × 100r, cap=64)",
            Arc::new(Pool::new(settings(64, std::time::Duration::from_secs(300)))),
            500,
            100,
            1,
            0,
        );
        assert!(pool_b <= 64, "B: pool {pool_b} exceeded cap 64");

        // C: Token rotation churn (5 rotations per user)
        let (tp_c, _, pool_c, _) = run_scenario(
            "C: token rotation (200u × 50r, 5 rot)",
            Arc::new(Pool::new(settings(
                512,
                std::time::Duration::from_secs(300),
            ))),
            200,
            50,
            5,
            0,
        );
        assert!(pool_c <= 512, "C: pool {pool_c} exceeded cap 512");

        // D: Slow init (10 ms) — verifies per-key serialization
        let (tp_d, elapsed_d, _, inits_d) = run_scenario(
            "D: slow init 10ms (100u × 20r, cap=256)",
            Arc::new(Pool::new(settings(
                256,
                std::time::Duration::from_secs(300),
            ))),
            100,
            20,
            1,
            10,
        );
        assert!(
            inits_d <= 100,
            "D: inits {inits_d} > users 100 — Mutex not serializing init correctly"
        );
        // Wall-clock should be << 100 users × 10ms = 1000ms if parallel
        eprintln!(
            "  D elapsed={elapsed_d:.3}s (serial would be ≥ {:.1}s, parallel < {:.1}s)",
            100.0 * 0.01,
            20.0 * 0.01
        );

        eprintln!("  {}", "─".repeat(72));
        eprintln!("  throughput: A={tp_a:.0} B={tp_b:.0} C={tp_c:.0} D={tp_d:.0} ops/s");
        eprintln!("══════════════════════════════════════════════════════════════════");

        assert!(tp_a > 1_000.0, "A throughput {tp_a:.0} ops/s too low");
        assert!(tp_b > 1_000.0, "B throughput {tp_b:.0} ops/s too low");
        assert!(
            elapsed_d < 5.0,
            "D took {elapsed_d:.2}s — inits may be serializing globally rather than per-key"
        );
    }
}
