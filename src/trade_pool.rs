//! WebSocket `TradeContext` pool — see [`crate::context_pool`] for the
//! generic caching/eviction engine this wraps.
//!
//! Mirrors [`crate::ws_pool`]'s `QuoteContext` pooling: without this,
//! every trade tool call opened a fresh WebSocket connection, unlike quote
//! calls which have always shared a pooled connection per identity.

use std::sync::{Arc, LazyLock, OnceLock};

use longbridge::trade::TradeContext;

use crate::context_pool::{
    Pool, PoolMetrics, PoolSettings, cache_key_for_token, ensure_sweeper_started, token_fingerprint,
};

const DEFAULT_IDLE_TTL_SECS: u64 = 10 * 60;
const DEFAULT_MAX_ENTRIES: usize = 1024;
const IDLE_TTL_ENV: &str = "LONGBRIDGE_MCP_TRADE_WS_IDLE_TTL_SECS";
const MAX_ENTRIES_ENV: &str = "LONGBRIDGE_MCP_TRADE_WS_MAX_CONTEXTS";

static POOL: LazyLock<Pool<TradeContext>> = LazyLock::new(|| {
    Pool::new(
        PoolSettings::from_env(
            IDLE_TTL_ENV,
            MAX_ENTRIES_ENV,
            DEFAULT_MAX_ENTRIES,
            DEFAULT_IDLE_TTL_SECS,
        ),
        PoolMetrics {
            record_event: crate::metrics::record_trade_ws_pool_event,
            set_entries: crate::metrics::set_trade_ws_pool_entries,
        },
    )
});
static SWEEPER_STARTED: OnceLock<()> = OnceLock::new();

/// Return the cached `TradeContext` for `token`, creating one on first use.
///
/// `make_config` is a lazy factory: it is only called on a cache miss so
/// callers avoid building a `Config` (and its `Arc` allocation) on every
/// cache hit. Pass `|| mctx.create_config()` rather than
/// `mctx.create_config()`.
pub async fn get_or_init_trade(
    token: &str,
    make_config: impl FnOnce() -> Arc<longbridge::Config>,
) -> TradeContext {
    ensure_sweeper_started(&POOL, &SWEEPER_STARTED);

    let key = cache_key_for_token(token);
    let token_fingerprint = token_fingerprint(token);
    POOL.get_or_insert_with(key, token_fingerprint, || {
        let (ctx, _) = TradeContext::new(make_config());
        ctx
    })
}

/// Evict the cached `TradeContext` for `token`. Call this after any error on
/// a trade API so the next request creates a fresh WebSocket connection
/// rather than reusing a potentially broken one.
pub fn evict(token: &str) {
    POOL.remove_identity(&cache_key_for_token(token));
}
