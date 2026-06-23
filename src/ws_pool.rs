//! WebSocket QuoteContext pool.
//!
//! `QuoteContext` opens a persistent WebSocket connection to Longbridge.
//! Creating one per tool call would exhaust the server-side per-account
//! connection limit under concurrent load.  This module caches one
//! `QuoteContext` per authenticated identity so all quote tool calls from the
//! same user in this process share a single connection.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use longbridge::quote::QuoteContext;
use sha2::{Digest, Sha256};

const DEFAULT_IDLE_TTL_SECS: u64 = 10 * 60;
const DEFAULT_MAX_ENTRIES: usize = 1024;
const IDLE_TTL_ENV: &str = "LONGBRIDGE_MCP_QUOTE_WS_IDLE_TTL_SECS";
const MAX_ENTRIES_ENV: &str = "LONGBRIDGE_MCP_QUOTE_WS_MAX_CONTEXTS";

static POOL: LazyLock<Pool<QuoteContext>> = LazyLock::new(|| Pool::new(PoolSettings::from_env()));
static SWEEPER_STARTED: OnceLock<()> = OnceLock::new();

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
}

struct Pool<T: Clone> {
    settings: PoolSettings,
    entries: Mutex<HashMap<PoolKey, PoolEntry<T>>>,
}

impl<T: Clone> Pool<T> {
    fn new(settings: PoolSettings) -> Self {
        Self {
            settings,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn get_or_insert_with(
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
        let mut entries = self.entries.lock().unwrap_or_else(|err| err.into_inner());
        let idle_evictions = self.prune_idle_locked(&mut entries, now);
        if idle_evictions > 0 {
            crate::metrics::record_quote_ws_pool_event("evict_idle", idle_evictions as u64);
        }

        if let Some(value) = entries.get_mut(&key).and_then(|entry| {
            if entry.token_fingerprint == token_fingerprint {
                entry.last_used = now;
                Some(entry.value.clone())
            } else {
                None
            }
        }) {
            crate::metrics::record_quote_ws_pool_event("hit", 1);
            crate::metrics::set_quote_ws_pool_entries(entries.len());
            return value;
        }

        if entries.contains_key(&key) {
            entries.remove(&key);
            crate::metrics::record_quote_ws_pool_event("evict_rotated_token", 1);
        }

        if entries.len() >= self.settings.max_entries {
            if self.evict_lru_locked(&mut entries).is_some() {
                crate::metrics::record_quote_ws_pool_event("evict_capacity", 1);
            }
        }

        crate::metrics::record_quote_ws_pool_event("miss", 1);
        let value = init();
        entries.insert(
            key,
            PoolEntry {
                value: value.clone(),
                token_fingerprint,
                last_used: now,
            },
        );
        crate::metrics::set_quote_ws_pool_entries(entries.len());
        value
    }

    fn remove_identity(&self, identity: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|err| err.into_inner());
        let before = entries.len();
        entries.retain(|key, _| key.identity != identity);
        let removed = before.saturating_sub(entries.len());
        if removed > 0 {
            crate::metrics::record_quote_ws_pool_event("evict_explicit", removed as u64);
            crate::metrics::set_quote_ws_pool_entries(entries.len());
        }
    }

    fn prune_idle_now(&self) {
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap_or_else(|err| err.into_inner());
        let removed = self.prune_idle_locked(&mut entries, now);
        if removed > 0 {
            crate::metrics::record_quote_ws_pool_event("evict_idle", removed as u64);
            crate::metrics::set_quote_ws_pool_entries(entries.len());
        }
    }

    fn prune_idle_locked(
        &self,
        entries: &mut HashMap<PoolKey, PoolEntry<T>>,
        now: Instant,
    ) -> usize {
        let before = entries.len();
        let idle_ttl = self.settings.idle_ttl;
        entries.retain(|_, entry| now.saturating_duration_since(entry.last_used) <= idle_ttl);
        before.saturating_sub(entries.len())
    }

    fn evict_lru_locked(&self, entries: &mut HashMap<PoolKey, PoolEntry<T>>) -> Option<PoolKey> {
        let key = entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone())?;
        entries.remove(&key);
        Some(key)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .len()
    }

    #[cfg(test)]
    fn contains_key(&self, key: &PoolKey) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .contains_key(key)
    }
}

/// Return the cached `QuoteContext` for `token`, creating one on first use.
pub async fn get_or_init_quote(token: &str, config: Arc<longbridge::Config>) -> QuoteContext {
    ensure_idle_sweeper_started();

    let key = cache_key_for_token(token);
    let token_fingerprint = token_fingerprint(token);
    POOL.get_or_insert_with(key, token_fingerprint, || {
        let (ctx, _) = QuoteContext::new(config);
        ctx
    })
}

/// Evict a cached entry when a token expires or auth fails.
#[allow(dead_code)]
pub fn evict(token: &str) {
    POOL.remove_identity(&cache_key_for_token(token).identity);
}

fn ensure_idle_sweeper_started() {
    SWEEPER_STARTED.get_or_init(|| {
        let interval = sweep_interval(POOL.settings.idle_ttl);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                POOL.prune_idle_now();
            }
        });
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
        assert!(pool.contains_key(&a));
        assert!(!pool.contains_key(&b));
        assert!(pool.contains_key(&c));
    }

    #[test]
    fn jwt_subject_key_survives_token_refresh_without_storing_plaintext() {
        let sub = r#"{"account_channel":"lb","user_id":"u-1"}"#;
        let token_a = jwt_with_sub(sub, "signature-a");
        let token_b = jwt_with_sub(sub, "signature-b");

        let key_a = cache_key_for_token(&token_a);
        let key_b = cache_key_for_token(&token_b);

        assert_eq!(key_a, key_b);
        assert!(!key_a.identity.contains(&token_a));
        assert!(!key_a.identity.contains(sub));
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
        assert!(pool.contains_key(&key));
    }
}
