//! Longbridge upstream endpoints, fixed once at process start.
//!
//! # Why the endpoints are pinned
//!
//! The SDK, left to itself, derives its base URLs at request time: it reads
//! `LONGBRIDGE_HTTP_URL` / `LONGBRIDGE_QUOTE_WS_URL` / `LONGBRIDGE_TRADE_WS_URL`
//! (plus the `LONGPORT_` aliases, and a `.env` file via `dotenv`), and when
//! none are set it probes `geotest.lbkrs.com` to choose between the
//! `*.longbridge.cn` and `*.longbridge.com` access points. The same binary then
//! talks to different hosts depending on where it runs and what happens to be
//! in the environment.
//!
//! This module removes that ambiguity: the environment is chosen once at
//! startup ([`init`], from `--canary` / the config file) and every upstream URL
//! is set explicitly on the SDK afterwards, so no environment variable and no
//! geolocation probe can influence it.
//!
//! # `.cn` is deliberately absent
//!
//! Mainland acceleration through `openapi.longbridge.cn` is not used. `.cn` has
//! no path to the US data center, so a `us_`-prefixed credential sent there
//! authenticates but fails every market-data request with
//! `301604 no quote access` — a failure that reads like a missing permission
//! and is not one. `.com` serves both data centers, so it is the only host.
//!
//! Host selection and data-center routing are two independent things: which
//! data center serves a request is decided by the `x-dc-region` header, which
//! the SDK derives from the credential's `us_` / `ap_` prefix. That is
//! unaffected by anything here.
//!
//! # Scope
//!
//! Everything this server sends upstream, plus the OAuth URLs it advertises and
//! the connect page it points users at, follows [`current`]. Static tool
//! metadata cannot: it is made of literals. Those name
//! [`STATIC_CONNECT_PAGE`] and are retargeted once at startup — which is why
//! [`init`] must run before the first `tools/list`.

use std::sync::OnceLock;

#[cfg(test)]
tokio::task_local! {
    /// Test-only override for the upstream HTTP and OAuth base URL, scoped
    /// around the code under test so it can be pointed at a local mock server.
    /// Replaces the process-wide env-var mutation this module exists to
    /// eliminate; being task-scoped, it needs no cross-test locking.
    ///
    /// Use `scope` when the override must survive `.await` points inside the
    /// code under test, `sync_scope` when only synchronous client construction
    /// happens inside it. Task-locals are **not** inherited by `tokio::spawn`,
    /// so a client must be built inside the scope, never in a task spawned
    /// from it.
    pub(crate) static UPSTREAM_OVERRIDE: String;
}

/// The Longbridge environment this process talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
    /// Production (`*.longbridge.com`).
    #[default]
    Production,
    /// Canary (`*.longbridge.xyz`).
    Canary,
}

impl Environment {
    /// OpenAPI REST base URL.
    pub fn http_url(self) -> &'static str {
        match self {
            Environment::Production => "https://openapi.longbridge.com",
            Environment::Canary => CANARY_GLOBAL_GATEWAY,
        }
    }

    /// Quote WebSocket endpoint.
    pub fn quote_ws_url(self) -> &'static str {
        match self {
            Environment::Production => "wss://openapi-quote.longbridge.com/v2",
            Environment::Canary => "wss://openapi-global-quote.longbridge.xyz/v2",
        }
    }

    /// Trade WebSocket endpoint.
    pub fn trade_ws_url(self) -> &'static str {
        match self {
            Environment::Production => "wss://openapi-trade.longbridge.com/v2",
            Environment::Canary => "wss://openapi-global-trade.longbridge.xyz/v2",
        }
    }

    /// OAuth base URL: the authorization server advertised in the RFC 8414 /
    /// RFC 9728 metadata and the host the `authenticate` tool exchanges codes
    /// against. The same host as [`Environment::http_url`] in both
    /// environments, kept as its own method so the two can diverge without
    /// touching callers.
    pub fn oauth_url(self) -> &'static str {
        match self {
            Environment::Production => "https://openapi.longbridge.com",
            Environment::Canary => CANARY_GLOBAL_GATEWAY,
        }
    }

    /// Page where a user generates a one-time authorization code.
    pub fn connect_page_url(self) -> &'static str {
        match self {
            Environment::Production => STATIC_CONNECT_PAGE,
            Environment::Canary => "https://open.longbridge.xyz/connect",
        }
    }

    /// `redirect_uri` the connect page binds authorization codes to. The token
    /// exchange must send this exact value; keep it in sync with the web
    /// `getAgentRedirectUri()` and the CLI.
    pub fn agent_redirect_uri(self) -> &'static str {
        match self {
            Environment::Production => "https://open.longbridge.com/connect/done",
            Environment::Canary => "https://open.longbridge.xyz/connect/done",
        }
    }

    /// Short label for logs and the startup banner.
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Production => "production",
            Environment::Canary => "canary",
        }
    }
}

/// Canary's global gateway — the counterpart of production's
/// `openapi.longbridge.com`.
///
/// Canary fronts the same backend with two gateways: this one, which is
/// CloudFront-fronted and performs `x-dc-region` data-center routing exactly
/// like production, and `openapi.longbridge.xyz`, which resolves to regional
/// (ap-east-1) addresses. This server serves `us_`- and `ap_`-prefixed
/// credentials from one process and depends on that routing, so the regional
/// endpoint would reproduce the `301604 no quote access` failure described
/// above. `longbridge-terminal` (`src/region.rs`) and the SDK's staging OAuth
/// base point at the same host.
const CANARY_GLOBAL_GATEWAY: &str = "https://openapi-global.longbridge.xyz";

/// The connect page baked into static tool metadata: the `#[tool]` description
/// and `auth_code` schema doc, which need literals, and the locale files.
///
/// It is the production URL and doubles as the placeholder those surfaces are
/// retargeted from when the process runs against canary — the same idiom as
/// `crate::auth::LANDING_PAGE_URL_PLACEHOLDER`. Retargeting them matters: left
/// alone, canary would send users to the production page while exchanging the
/// resulting code against canary, and the `redirect_uri` mismatch surfaces as
/// an opaque `invalid_grant`.
pub const STATIC_CONNECT_PAGE: &str = "https://open.longbridge.com/connect";

/// An OAuth scope this server advertises in its RFC 8414 / RFC 9728 metadata.
///
/// The authorization server identifies scopes by *numeric id* in the `scope`
/// request parameter, and those ids are environment-specific: production uses
/// `4/6/10/11` while canary uses `18/20/21/24` for the same four concepts
/// (observed from dynamic client registration, which reports each
/// environment's available set — `4 6 10 11 12` vs `18 20 21 24 25`). The
/// stable identifier is the `key`, which is what the token endpoint echoes back
/// and what `data/scopes.json` records; the numeric id is resolved per
/// environment by [`Scope::id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Watchlist group management.
    Watchlist,
    /// Account assets, positions, and cash flow (read-only).
    AccountRead,
    /// Order and execution lookup (read-only).
    TradeRead,
    /// Order placement, DCA, and grid writes.
    TradeWrite,
}

impl Scope {
    /// Stable scope key, matching the `key` field in `data/scopes.json` and the
    /// `scope` value the token endpoint returns.
    pub fn key(self) -> &'static str {
        match self {
            Scope::Watchlist => "watchlist",
            Scope::AccountRead => "account.read",
            Scope::TradeRead => "trade.read",
            Scope::TradeWrite => "trade.write",
        }
    }

    /// Numeric id the given environment's authorization server expects.
    pub fn id(self, environment: Environment) -> &'static str {
        match (self, environment) {
            (Scope::Watchlist, Environment::Production) => "4",
            (Scope::AccountRead, Environment::Production) => "6",
            (Scope::TradeRead, Environment::Production) => "10",
            (Scope::TradeWrite, Environment::Production) => "11",
            (Scope::Watchlist, Environment::Canary) => "18",
            (Scope::AccountRead, Environment::Canary) => "20",
            (Scope::TradeRead, Environment::Canary) => "21",
            (Scope::TradeWrite, Environment::Canary) => "24",
        }
    }
}

/// Scopes advertised on the full endpoints (`/mcp`, `/agent`, root).
pub const SCOPES: &[Scope] = &[
    Scope::Watchlist,
    Scope::AccountRead,
    Scope::TradeRead,
    Scope::TradeWrite,
];

/// Scopes advertised on the restricted `/v2` endpoint: the read-only subset,
/// deliberately without [`Scope::TradeWrite`].
pub const V2_SCOPES: &[Scope] = &[Scope::Watchlist, Scope::AccountRead, Scope::TradeRead];

/// Scope ids to advertise for the current environment.
pub fn scope_ids(scopes: &[Scope]) -> Vec<String> {
    let environment = current();
    scopes
        .iter()
        .map(|scope| scope.id(environment).to_string())
        .collect()
}

static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();

/// Fix the environment for this process. Called once during startup, before any
/// upstream client is built.
///
/// # Panics
///
/// Panics if called more than once — the endpoints must not change under a
/// running server.
pub fn init(environment: Environment) {
    ENVIRONMENT
        .set(environment)
        .expect("endpoints::init called more than once");
    // Logged here rather than by the caller so stdio mode — which prints no
    // startup banner — still records which environment it is talking to.
    tracing::info!(
        environment = environment.as_str(),
        http_url = environment.http_url(),
        quote_ws_url = environment.quote_ws_url(),
        trade_ws_url = environment.trade_ws_url(),
        oauth_url = environment.oauth_url(),
        "upstream endpoints fixed"
    );
}

/// The environment fixed by [`init`], or [`Environment::Production`] when
/// nothing set it (unit tests, and any path that skips startup configuration).
pub fn current() -> Environment {
    ENVIRONMENT.get().copied().unwrap_or_default()
}

#[cfg(test)]
fn override_url() -> Option<String> {
    UPSTREAM_OVERRIDE.try_with(Clone::clone).ok()
}

#[cfg(not(test))]
fn override_url() -> Option<String> {
    None
}

/// Resolve a base URL, honoring the test override, without a trailing slash so
/// callers can append a leading-slash path.
fn base_url(default: &'static str) -> String {
    let url = override_url().unwrap_or_else(|| default.to_string());
    url.trim_end_matches('/').to_string()
}

/// OpenAPI REST base URL for the current environment.
pub fn http_url() -> String {
    base_url(current().http_url())
}

/// OAuth base URL for the current environment.
pub fn oauth_url() -> String {
    base_url(current().oauth_url())
}

/// Quote WebSocket endpoint for the current environment.
pub fn quote_ws_url() -> &'static str {
    current().quote_ws_url()
}

/// Trade WebSocket endpoint for the current environment.
pub fn trade_ws_url() -> &'static str {
    current().trade_ws_url()
}

/// Connect-page URL for the current environment.
pub fn connect_page_url() -> &'static str {
    current().connect_page_url()
}

/// Agent `redirect_uri` for the current environment.
pub fn agent_redirect_uri() -> &'static str {
    current().agent_redirect_uri()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every host is spelled out here so a typo in a TLD fails the build rather
    /// than silently pointing production at canary or vice versa.
    #[test]
    fn endpoint_table() {
        let cases = [
            (
                Environment::Production,
                "https://openapi.longbridge.com",
                "wss://openapi-quote.longbridge.com/v2",
                "wss://openapi-trade.longbridge.com/v2",
                "https://openapi.longbridge.com",
                "https://open.longbridge.com/connect",
                "https://open.longbridge.com/connect/done",
            ),
            (
                Environment::Canary,
                "https://openapi-global.longbridge.xyz",
                "wss://openapi-global-quote.longbridge.xyz/v2",
                "wss://openapi-global-trade.longbridge.xyz/v2",
                "https://openapi-global.longbridge.xyz",
                "https://open.longbridge.xyz/connect",
                "https://open.longbridge.xyz/connect/done",
            ),
        ];

        for (env, http, quote_ws, trade_ws, oauth, connect, redirect) in cases {
            assert_eq!(env.http_url(), http, "http_url for {env:?}");
            assert_eq!(env.quote_ws_url(), quote_ws, "quote_ws_url for {env:?}");
            assert_eq!(env.trade_ws_url(), trade_ws, "trade_ws_url for {env:?}");
            assert_eq!(env.oauth_url(), oauth, "oauth_url for {env:?}");
            assert_eq!(
                env.connect_page_url(),
                connect,
                "connect_page_url for {env:?}"
            );
            assert_eq!(
                env.agent_redirect_uri(),
                redirect,
                "agent_redirect_uri for {env:?}"
            );
        }
    }

    /// Catches the realistic slip: a `.com` const copy-pasted into the canary
    /// arm, or vice versa.
    #[test]
    fn every_url_matches_its_environment_domain() {
        for env in [Environment::Production, Environment::Canary] {
            let expected = match env {
                Environment::Production => "longbridge.com",
                Environment::Canary => "longbridge.xyz",
            };
            for url in [
                env.http_url(),
                env.quote_ws_url(),
                env.trade_ws_url(),
                env.oauth_url(),
                env.connect_page_url(),
                env.agent_redirect_uri(),
            ] {
                assert!(
                    url.contains(expected),
                    "{env:?} URL {url} must be on {expected}"
                );
            }
        }
    }

    /// Callers append leading-slash paths (`{base}/oauth2/token`), so a
    /// trailing slash here would produce a double slash upstream.
    #[test]
    fn base_urls_have_no_trailing_slash() {
        for env in [Environment::Production, Environment::Canary] {
            assert!(!env.http_url().ends_with('/'), "http_url for {env:?}");
            assert!(!env.oauth_url().ends_with('/'), "oauth_url for {env:?}");
        }
    }

    /// Guard: the production ids in [`Scope::id`] must match `data/scopes.json`,
    /// which is the catalogue the public `/mcp/scopes.json` manifest and the
    /// translations are built from. Keys are the stable identifier, so this
    /// pins the id-to-concept mapping against the only other place that
    /// records it.
    #[test]
    fn production_scope_ids_match_the_catalogue() {
        let catalogue: serde_json::Value =
            serde_json::from_str(include_str!("../data/scopes.json"))
                .expect("data/scopes.json must be valid JSON");
        let entries = catalogue["scopes"]
            .as_array()
            .expect("scopes.json must hold a `scopes` array");
        for scope in SCOPES {
            let entry = entries
                .iter()
                .find(|e| e["key"].as_str() == Some(scope.key()))
                .unwrap_or_else(|| panic!("no `{}` entry in data/scopes.json", scope.key()));
            assert_eq!(
                entry["id"].as_str(),
                Some(scope.id(Environment::Production)),
                "production id for `{}` disagrees with data/scopes.json",
                scope.key()
            );
        }
    }

    /// Every environment must map the four concepts onto four distinct ids, and
    /// the `/v2` set must never carry trade execution.
    #[test]
    fn scope_sets_are_well_formed() {
        for env in [Environment::Production, Environment::Canary] {
            let ids: Vec<&str> = SCOPES.iter().map(|s| s.id(env)).collect();
            let mut unique = ids.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(
                unique.len(),
                ids.len(),
                "duplicate scope id for {env:?}: {ids:?}"
            );
        }
        assert!(
            !V2_SCOPES.contains(&Scope::TradeWrite),
            "/v2 is the read-only surface and must not advertise trade execution"
        );
        assert!(
            V2_SCOPES.iter().all(|s| SCOPES.contains(s)),
            "/v2 scopes must be a subset of the full set"
        );
    }

    #[test]
    fn defaults_to_production_without_init() {
        assert_eq!(
            current(),
            Environment::Production,
            "an uninitialized process must talk to production"
        );
    }

    /// The whole point of the module: the resolved base URL comes from the
    /// environment enum alone. No `LONGBRIDGE_HTTP_URL` read is involved, so no
    /// value of that variable can change the outcome — asserted here rather
    /// than by mutating the process environment, which would race the other
    /// tests' `getenv` calls.
    #[test]
    fn base_url_comes_from_the_environment_enum() {
        assert_eq!(
            http_url(),
            current().http_url(),
            "http_url must be exactly the current environment's host"
        );
        assert_eq!(
            oauth_url(),
            current().oauth_url(),
            "oauth_url must be exactly the current environment's host"
        );
    }

    #[tokio::test]
    async fn test_override_replaces_http_and_oauth_base() {
        let scoped = UPSTREAM_OVERRIDE
            .scope("http://127.0.0.1:9/".to_string(), async {
                (http_url(), oauth_url())
            })
            .await;
        assert_eq!(
            scoped,
            (
                "http://127.0.0.1:9".to_string(),
                "http://127.0.0.1:9".to_string()
            ),
            "the override must cover both bases and drop the trailing slash"
        );
        assert_eq!(
            http_url(),
            Environment::Production.http_url(),
            "the override must not leak outside its scope"
        );
    }
}
