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

/// Derives `scheme://host` from the incoming request headers.
///
/// Header priority for the host:
///   1. `X-Forwarded-Host` — set by the reverse proxy to the external hostname
///      (e.g. `openapi.longbridge.xyz` when the proxy rewrites the Host).
///   2. `Host` — the hostname the client actually connected to (correct for
///      direct connections; may be the internal backend host behind a proxy).
///   3. Falls back to `fallback` (`--base-url`) when both are absent.
///
/// Scheme priority: `X-Forwarded-Proto` → scheme of `--base-url`.
pub(crate) fn resource_url_from_headers(headers: &HeaderMap, fallback: &str) -> String {
    let Some(host) = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
    else {
        return fallback.to_string();
    };
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
    format!("{scheme}://{host}")
}

#[derive(Serialize)]
pub(crate) struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

/// Scope value advertised for the restricted public `/v2` endpoint.
///
/// A client discovering auth from a `/v2` 401 copies this `mcp-endpoint=v2`
/// marker into the authorization request's `scope` parameter, so the Longbridge
/// authorization server can present the broader read-only consent set for `/v2`
/// (account/portfolio + order history, but no trade execution, DCA, IPO orders,
/// or money movement). It is a marker, not a granular OAuth scope list —
/// narrowing the granted permissions is done server-side off this marker.
const V2_SCOPES_SUPPORTED: &[&str] = &["mcp-endpoint=v2"];

fn build_resource_metadata(resource: String, scopes: Vec<String>) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource,
        authorization_servers: vec![longbridge_oauth_url()],
        scopes_supported: scopes,
    }
}

pub async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ProtectedResourceMetadata> {
    let resource = resource_url_from_headers(&headers, &state.base_url);
    Json(build_resource_metadata(
        resource,
        vec!["openapi".to_string()],
    ))
}

/// Protected-resource metadata for the restricted `/v2` endpoint (RFC 9728
/// resource-specific document at `/.well-known/oauth-protected-resource/v2`).
///
/// The `resource` identifier is the `/v2` URL and `scopes_supported` is the
/// `/v2` read-only marker, so a client that discovers auth from a `/v2` 401
/// requests the v2 consent set — keeping trade execution off the consent screen.
pub async fn protected_resource_metadata_v2(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ProtectedResourceMetadata> {
    let resource = format!(
        "{}/v2",
        resource_url_from_headers(&headers, &state.base_url)
    );
    let scopes = V2_SCOPES_SUPPORTED.iter().map(|s| s.to_string()).collect();
    Json(build_resource_metadata(resource, scopes))
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
