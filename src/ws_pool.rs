//! WebSocket connection pool — plan A + B.
//!
//! **Plan A (cache):** one `QuoteContext` + `TradeContext` +
//! `FundamentalContext` per OAuth token, reused across all tool calls from the
//! same user session.  Eliminates the O(n) connection-open overhead that caused
//! CPU spikes when many tool calls arrived concurrently.
//!
//! **Plan B (semaphore):** at most `MAX_INIT_CONCURRENCY` connections may be
//! *opened* simultaneously.  Once a context is cached the semaphore is not
//! needed; it only guards the cold-start path so that a burst of first-time
//! requests can't punch through the Longbridge server-side limit of 10
//! concurrent WS connections per account.

use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use longbridge::fundamental::FundamentalContext;
use longbridge::quote::QuoteContext;
use longbridge::trade::TradeContext;
use tokio::sync::Semaphore;

/// Maximum number of new WS connections that may be established concurrently.
/// Set below the Longbridge server-side limit (10) to leave headroom for other
/// SDK users (CLI, mobile) sharing the same account.
const MAX_INIT_CONCURRENCY: usize = 5;

#[derive(Clone)]
pub struct CachedContexts {
    pub quote: QuoteContext,
    pub trade: TradeContext,
    pub fundamental: FundamentalContext,
}

struct Pool {
    /// Cache keyed by OAuth access token (the full JWT string).
    cache: DashMap<String, CachedContexts>,
    /// Plan B: limits concurrent cold-start connection creation.
    init_sem: Semaphore,
}

static POOL: LazyLock<Pool> = LazyLock::new(|| Pool {
    cache: DashMap::new(),
    init_sem: Semaphore::new(MAX_INIT_CONCURRENCY),
});

/// Return cached contexts for `token`, creating them (under the semaphore) if
/// this is the first call for this token.
pub async fn get_or_init(token: &str, config: Arc<longbridge::Config>) -> CachedContexts {
    // Fast path: already cached.
    if let Some(ctx) = POOL.cache.get(token) {
        return ctx.clone();
    }

    // Slow path: acquire a permit so at most MAX_INIT_CONCURRENCY connections
    // are created simultaneously (plan B).
    let _permit = POOL.init_sem.acquire().await;

    // Re-check after acquiring the permit — another task may have cached it
    // while we were waiting.
    if let Some(ctx) = POOL.cache.get(token) {
        return ctx.clone();
    }

    let (quote, _) = QuoteContext::new(Arc::clone(&config));
    let (trade, _) = TradeContext::new(Arc::clone(&config));
    let fundamental = FundamentalContext::new(Arc::clone(&config));

    let ctx = CachedContexts {
        quote,
        trade,
        fundamental,
    };
    POOL.cache.insert(token.to_string(), ctx.clone());
    ctx
}

/// Evict a cached entry (e.g. when a token expires or auth fails).
pub fn evict(token: &str) {
    POOL.cache.remove(token);
}

/// Number of tokens currently cached (for metrics / diagnostics).
pub fn cached_count() -> usize {
    POOL.cache.len()
}
