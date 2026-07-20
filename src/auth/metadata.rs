use std::sync::{Arc, LazyLock};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;
use serde::Serialize;

use crate::auth::AppState;

fn longbridge_oauth_url() -> String {
    std::env::var("LONGBRIDGE_HTTP_URL")
        .unwrap_or_else(|_| "https://openapi.longbridge.com".to_string())
}

/// Authorization-server URL advertised to clients that reached us through the
/// global single-domain entry (allowlisted `X-Host`, see [`public_hosts`]).
/// Unset/empty = such requests keep advertising [`longbridge_oauth_url`].
///
/// Deliberately separate from `LONGBRIDGE_HTTP_URL`: that variable is also the
/// longbridge SDK upstream base and the `/agent` reverse-auth base, so pointing
/// it at the global edge domain would reroute this server's own upstream calls
/// through the public edge.
fn global_oauth_url() -> Option<String> {
    std::env::var("LONGBRIDGE_GLOBAL_OAUTH_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Hostnames that may be asserted via the edge-injected `X-Host` header
/// (comma-separated env `LONGBRIDGE_PUBLIC_HOSTS`). Unset/empty = the `X-Host`
/// path is disabled entirely and behavior is unchanged.
///
/// The allowlist is what makes `X-Host` trustworthy: the DC ingress rewrites
/// `Host`/`X-Forwarded-Host` to the origin hostname but passes custom headers
/// through — from the edge *and* from anyone hitting the origin directly.
/// Without the gate, a direct caller could make the 401 challenge / RFC 9728
/// metadata advertise an attacker-controlled domain.
fn public_hosts() -> Vec<String> {
    std::env::var("LONGBRIDGE_PUBLIC_HOSTS")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Public URL resolved for a request, plus whether the host was asserted
/// through the global single-domain entry (allowlisted `X-Host`).
pub(crate) struct PublicUrl {
    /// `scheme://host` to echo in the 401 challenge and RFC 9728 metadata.
    pub url: String,
    /// True when the host came from an allowlisted `X-Host`; switches the
    /// advertised authorization server to [`global_oauth_url`].
    pub via_global_entry: bool,
}

/// Derives the public `scheme://host` a request should be answered with.
///
/// Header priority for the host:
///   1. `X-Host` — injected by the global edge (Lambda@Edge) with the single
///      public domain (e.g. `mcp-global.longbridge.xyz`); only honoured when it
///      matches the `LONGBRIDGE_PUBLIC_HOSTS` allowlist. Needed because the DC
///      ingress rewrites `Host`/`X-Forwarded-Host` back to the origin hostname,
///      while custom headers pass through untouched.
///   2. `X-Forwarded-Host` — set by the reverse proxy to the external hostname
///      (e.g. `openapi.longbridge.xyz` when the proxy rewrites the Host).
///   3. `Host` — the hostname the client actually connected to (correct for
///      direct connections; may be the internal backend host behind a proxy).
///   4. Falls back to `fallback` (`--base-url`) when all are absent.
///
/// Scheme priority: `X-Forwarded-Proto` → scheme of `--base-url`.
pub(crate) fn public_url_from_headers(headers: &HeaderMap, fallback: &str) -> PublicUrl {
    resolve_public_url(headers, fallback, &public_hosts())
}

fn resolve_public_url(
    headers: &HeaderMap,
    fallback: &str,
    allowed_x_hosts: &[String],
) -> PublicUrl {
    // Prefer the proxy-set header; fall back to the scheme in --base-url so
    // that local HTTP deployments without a reverse proxy still return "http".
    let fallback_scheme = if fallback.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(fallback_scheme);

    let x_host = headers
        .get("x-host")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|h| !h.is_empty());
    if let Some(host) = x_host
        && allowed_x_hosts.iter().any(|a| a.eq_ignore_ascii_case(host))
    {
        return PublicUrl {
            url: format!("{scheme}://{host}"),
            via_global_entry: true,
        };
    }

    let Some(host) = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
    else {
        return PublicUrl {
            url: fallback.to_string(),
            via_global_entry: false,
        };
    };
    PublicUrl {
        url: format!("{scheme}://{host}"),
        via_global_entry: false,
    }
}

#[derive(Serialize)]
pub(crate) struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

/// OAuth scope ids advertised by the Longbridge authorization server.
///
/// - 4: Watchlist management, including creating, updating, and deleting user
///   watchlists.
/// - 6: Account assets and cash details, including fund/stock positions, cash
///   balances, and cash-flow history for portfolio overview and reconciliation.
/// - 10: Trade order lookup, covering read-only order lifecycle and execution
///   reports, plus pre-order buying-power estimates.
/// - 11: Trade execution, including submit/replace/cancel orders and recurring
///   investment operations.
const SCOPES_SUPPORTED: &[&str] = &["4", "6", "10", "11"];
const V2_SCOPES_SUPPORTED: &[&str] = &["4", "6", "10"];

/// Picks the authorization server to advertise: requests that came through the
/// global single-domain entry get [`global_oauth_url`] (when configured) so the
/// whole OAuth bootstrap stays on the global domains; everything else keeps the
/// per-DC [`longbridge_oauth_url`].
pub(crate) fn select_authorization_server(
    via_global_entry: bool,
    global: Option<String>,
    fallback: String,
) -> String {
    match (via_global_entry, global) {
        (true, Some(url)) => url,
        _ => fallback,
    }
}

fn build_resource_metadata(
    authorization_server: String,
    resource: String,
    scopes_supported: &[&str],
) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource,
        authorization_servers: vec![authorization_server],
        scopes_supported: scopes_supported
            .iter()
            .map(|scope| scope.to_string())
            .collect(),
    }
}

pub async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ProtectedResourceMetadata> {
    let public = public_url_from_headers(&headers, &state.base_url);
    Json(build_resource_metadata(
        public.url.clone(),
        public.url,
        SCOPES_SUPPORTED,
    ))
}

/// Protected-resource metadata for the restricted `/v2` endpoint (RFC 9728
/// resource-specific document at `/.well-known/oauth-protected-resource/v2`).
///
/// The `resource` identifier is the `/v2` URL. Scopes stay aligned with the
/// authorization server metadata instead of advertising an endpoint marker.
/// Scope 11 is intentionally excluded because `/v2` must not request trade
/// execution permissions.
pub async fn protected_resource_metadata_v2(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ProtectedResourceMetadata> {
    let public = public_url_from_headers(&headers, &state.base_url);
    let resource = format!("{}/v2", public.url);
    Json(build_resource_metadata(
        public.url,
        resource,
        V2_SCOPES_SUPPORTED,
    ))
}

/// Longbridge OAuth upstream selected for this public entry point.
pub(crate) fn oauth_upstream_url(via_global_entry: bool) -> String {
    select_authorization_server(via_global_entry, global_oauth_url(), longbridge_oauth_url())
}

/// RFC 8414 metadata for the minimal OAuth facade hosted by this MCP server.
/// Authorization and registration remain direct Longbridge endpoints; only
/// token exchange and revocation return through this server so it can attach
/// Longbridge's private DC routing header.
#[derive(Serialize)]
pub(crate) struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: String,
    registration_endpoint: String,
    jwks_uri: String,
    scopes_supported: Vec<String>,
    response_types_supported: Vec<&'static str>,
    grant_types_supported: Vec<&'static str>,
    token_endpoint_auth_methods_supported: Vec<&'static str>,
    code_challenge_methods_supported: Vec<&'static str>,
}

fn build_authorization_server_metadata(
    issuer: String,
    upstream: String,
) -> AuthorizationServerMetadata {
    let issuer = issuer.trim_end_matches('/');
    let upstream = upstream.trim_end_matches('/');
    AuthorizationServerMetadata {
        issuer: issuer.to_string(),
        authorization_endpoint: format!("{upstream}/oauth2/authorize"),
        token_endpoint: format!("{issuer}/oauth2/token"),
        revocation_endpoint: format!("{issuer}/oauth2/revoke"),
        registration_endpoint: format!("{upstream}/oauth2/register"),
        jwks_uri: format!("{upstream}/.well-known/jwks.json"),
        scopes_supported: SCOPES_SUPPORTED.iter().map(|s| s.to_string()).collect(),
        response_types_supported: vec!["code"],
        grant_types_supported: vec!["authorization_code", "refresh_token"],
        token_endpoint_auth_methods_supported: vec!["none"],
        code_challenge_methods_supported: vec!["S256", "plain"],
    }
}

/// Serve the MCP OAuth facade's RFC 8414 authorization-server metadata.
pub async fn authorization_server_metadata(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<AuthorizationServerMetadata> {
    let public = public_url_from_headers(&headers, &state.base_url);
    let upstream = oauth_upstream_url(public.via_global_entry);
    Json(build_authorization_server_metadata(public.url, upstream))
}

#[derive(Serialize)]
struct ServerInfoCard {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct AuthCard {
    required: bool,
    schemes: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct ServerCard {
    #[serde(rename = "serverInfo")]
    server_info: ServerInfoCard,
    authentication: AuthCard,
    tools: Vec<rmcp::model::Tool>,
}

static SERVER_CARD: LazyLock<ServerCard> = LazyLock::new(|| ServerCard {
    server_info: ServerInfoCard {
        name: "Longbridge MCP",
        version: env!("CARGO_PKG_VERSION"),
    },
    authentication: AuthCard {
        required: true,
        schemes: vec!["oauth2"],
    },
    tools: crate::tools::list_tools(),
});

/// Static MCP server card served at `/.well-known/mcp/server-card.json`.
///
/// Lets directory scanners (e.g. Smithery) discover server metadata and the
/// full tool list without performing the authenticated `tools/list` probe.
/// Declaring `authentication.schemes = ["oauth2"]` signals that the client
/// should follow the RFC 9728 protected-resource-metadata flow rather than
/// attempting Dynamic Client Registration directly.
pub async fn server_card() -> Json<&'static ServerCard> {
    Json(&*SERVER_CARD)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://localhost:8000";

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::try_from(*k).expect("valid header name"),
                v.parse().expect("valid header value"),
            );
        }
        h
    }

    fn allow(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn x_host_in_allowlist_wins_over_rewritten_host() {
        // Mirrors the production chain: the DC ingress has rewritten Host/XFH
        // back to the origin hostname; only the edge-injected X-Host still
        // carries the global single domain.
        let h = headers(&[
            ("x-host", "mcp-global.longbridge.xyz"),
            ("x-forwarded-host", "mcp.longbridge.xyz"),
            ("host", "mcp.longbridge.xyz"),
            ("x-forwarded-proto", "https"),
        ]);
        let got = resolve_public_url(&h, BASE, &allow(&["mcp-global.longbridge.xyz"]));
        assert_eq!(got.url, "https://mcp-global.longbridge.xyz");
        assert!(got.via_global_entry);
    }

    #[test]
    fn x_host_not_in_allowlist_is_ignored() {
        // A forged X-Host on a direct-to-origin request must not change the
        // echoed domain.
        let h = headers(&[
            ("x-host", "evil.example.com"),
            ("host", "mcp.longbridge.xyz"),
            ("x-forwarded-proto", "https"),
        ]);
        let got = resolve_public_url(&h, BASE, &allow(&["mcp-global.longbridge.xyz"]));
        assert_eq!(got.url, "https://mcp.longbridge.xyz");
        assert!(!got.via_global_entry);
    }

    #[test]
    fn x_host_disabled_when_allowlist_empty() {
        // LONGBRIDGE_PUBLIC_HOSTS unset = the feature is off entirely and
        // behavior matches the previous release.
        let h = headers(&[
            ("x-host", "mcp-global.longbridge.xyz"),
            ("host", "mcp.longbridge.xyz"),
            ("x-forwarded-proto", "https"),
        ]);
        let got = resolve_public_url(&h, BASE, &[]);
        assert_eq!(got.url, "https://mcp.longbridge.xyz");
        assert!(!got.via_global_entry);
    }

    #[test]
    fn x_host_match_is_case_insensitive() {
        let h = headers(&[
            ("x-host", "MCP-Global.Longbridge.XYZ"),
            ("host", "mcp.longbridge.xyz"),
            ("x-forwarded-proto", "https"),
        ]);
        let got = resolve_public_url(&h, BASE, &allow(&["mcp-global.longbridge.xyz"]));
        assert_eq!(got.url, "https://MCP-Global.Longbridge.XYZ");
        assert!(got.via_global_entry);
    }

    #[test]
    fn falls_back_to_xfh_then_host_then_base_url() {
        let allowlist = allow(&["mcp-global.longbridge.xyz"]);

        let xfh = headers(&[
            ("x-forwarded-host", "public.example.com"),
            ("host", "backend.internal"),
        ]);
        let got = resolve_public_url(&xfh, BASE, &allowlist);
        assert_eq!(got.url, "https://public.example.com");
        assert!(!got.via_global_entry);

        let host_only = headers(&[("host", "backend.internal")]);
        assert_eq!(
            resolve_public_url(&host_only, BASE, &allowlist).url,
            "https://backend.internal"
        );

        let empty = headers(&[]);
        assert_eq!(resolve_public_url(&empty, BASE, &allowlist).url, BASE);
    }

    #[test]
    fn scheme_prefers_x_forwarded_proto_then_base_url_scheme() {
        let allowlist = allow(&["mcp-global.longbridge.xyz"]);
        let h = headers(&[
            ("x-host", "mcp-global.longbridge.xyz"),
            ("x-forwarded-proto", "http"),
        ]);
        assert_eq!(
            resolve_public_url(&h, BASE, &allowlist).url,
            "http://mcp-global.longbridge.xyz"
        );

        let no_proto = headers(&[("x-host", "mcp-global.longbridge.xyz")]);
        assert_eq!(
            resolve_public_url(&no_proto, "http://localhost:8000", &allowlist).url,
            "http://mcp-global.longbridge.xyz"
        );
    }

    #[test]
    fn authorization_server_selection_matrix() {
        let global = || Some("https://openapi-global.longbridge.xyz".to_string());
        let fallback = || "https://openapi.longbridge.xyz".to_string();

        // Global entry + configured → advertise the global AS.
        assert_eq!(
            select_authorization_server(true, global(), fallback()),
            "https://openapi-global.longbridge.xyz"
        );
        // Global entry but LONGBRIDGE_GLOBAL_OAUTH_URL unset → keep the default.
        assert_eq!(
            select_authorization_server(true, None, fallback()),
            fallback()
        );
        // Direct-to-origin (not the global entry) → always the default,
        // existing clients see no change.
        assert_eq!(
            select_authorization_server(false, global(), fallback()),
            fallback()
        );
    }

    #[test]
    fn oauth_facade_only_proxies_credential_bearing_endpoints() {
        let metadata = build_authorization_server_metadata(
            "https://mcp.longbridge.com/".to_string(),
            "https://openapi.longbridge.com/".to_string(),
        );

        assert_eq!(metadata.issuer, "https://mcp.longbridge.com");
        assert_eq!(
            metadata.authorization_endpoint,
            "https://openapi.longbridge.com/oauth2/authorize"
        );
        assert_eq!(
            metadata.registration_endpoint,
            "https://openapi.longbridge.com/oauth2/register"
        );
        assert_eq!(
            metadata.token_endpoint,
            "https://mcp.longbridge.com/oauth2/token"
        );
        assert_eq!(
            metadata.revocation_endpoint,
            "https://mcp.longbridge.com/oauth2/revoke"
        );
    }
}
