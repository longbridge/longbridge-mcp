//! WebSocket QuoteContext pool.
//!
//! `QuoteContext` opens a persistent WebSocket connection to Longbridge.
//! Creating one per tool call would exhaust the server-side per-account
//! connection limit under concurrent load.  This module caches one
//! `QuoteContext` per OAuth token so all tool calls from the same session
//! share a single connection.

use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use longbridge::quote::QuoteContext;

static POOL: LazyLock<DashMap<String, QuoteContext>> = LazyLock::new(DashMap::new);

/// Return the cached `QuoteContext` for `token`, creating one on first use.
pub async fn get_or_init_quote(token: &str, config: Arc<longbridge::Config>) -> QuoteContext {
    if let Some(ctx) = POOL.get(token) {
        return ctx.clone();
    }
    let (ctx, _) = QuoteContext::new(config);
    POOL.insert(token.to_string(), ctx.clone());
    ctx
}

/// Evict a cached entry when a token expires or auth fails.
#[allow(dead_code)]
pub fn evict(token: &str) {
    POOL.remove(token);
}
