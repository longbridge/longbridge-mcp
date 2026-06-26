use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Bearer token extracted from the Authorization header.
#[derive(Clone, Debug)]
pub struct BearerToken(pub String);

/// Marker inserted into request extensions for requests that arrived on the
/// optional-auth `/agent` endpoint. Its presence tells downstream handlers
/// (`ServerHandler::list_tools`, `extract_context`) that a token-less request
/// is legitimate and should be steered into the `authenticate` reverse-auth
/// flow rather than treated as a hard error.
///
/// It is **never** inserted on the main MCP endpoint, so the main endpoint's
/// behaviour is unchanged: token-less requests are rejected with 401 before
/// they ever reach a handler.
#[derive(Clone, Debug)]
pub struct AgentEndpoint;

/// Which restricted public endpoint a request arrived on. Each maps to a
/// distinct tool allowlist and its own RFC 9728 resource-specific metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestrictedVersion {
    /// `/v2` — broader read surface (adds read-only account/portfolio,
    /// order/execution history, IPO market data, watchlist, alerts, sharelist,
    /// community) but still no trade execution, DCA, IPO orders, or money
    /// movement.
    V2,
}

impl RestrictedVersion {
    /// RFC 9728 protected-resource-metadata path advertised to clients that hit
    /// a `401` on this endpoint, so the authorize URL requests the matching
    /// read-only consent set.
    pub fn metadata_path(self) -> &'static str {
        match self {
            RestrictedVersion::V2 => "/.well-known/oauth-protected-resource/v2",
        }
    }
}

/// Marker inserted into request extensions for requests that arrived on a
/// restricted public endpoint (`/v2`). Its presence — and the
/// [`RestrictedVersion`] it carries — tells downstream handlers
/// (`ServerHandler::list_tools`, `ServerHandler::call_tool`) to expose and
/// accept only that version's allowlist, never trade execution, DCA, IPO
/// orders, or money-movement tools.
///
/// Unlike [`AgentEndpoint`], this is inserted regardless of token presence: the
/// restricted endpoints use [`AuthMode::Required`], so a valid Bearer token is
/// always present by the time the request reaches a handler, yet the exposed
/// tool set must still be restricted.
#[derive(Clone, Copy, Debug)]
pub struct RestrictedEndpoint(pub RestrictedVersion);

/// Which endpoint a request arrived on, which decides how token-less requests
/// are handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    /// Main MCP endpoint. Bearer token is **required**: token-less requests are
    /// rejected with `401` + `WWW-Authenticate`, exactly as the standard MCP
    /// OAuth 2.1 client flow expects.
    Required,
    /// Optional-auth `/agent` endpoint. Token-less requests are allowed through
    /// so an OAuth-incapable client can complete the handshake and call the
    /// `authenticate` tool. A valid Bearer token makes the endpoint behave
    /// exactly like the main endpoint (full tool set).
    Optional,
}

/// Auth middleware for MCP endpoints.
///
/// Extracts the Bearer token from the `Authorization` header and stores it as a
/// `BearerToken` in request extensions. No JWT validation -- the token is
/// forwarded to Longbridge SDK calls directly.
///
/// On 401 responses, includes `resource_metadata` in the `WWW-Authenticate`
/// header as required by the MCP OAuth 2.1 spec (RFC 9728).
///
/// ## Two modes
///
/// - [`AuthMode::Required`] (main endpoint): token-less requests are rejected
///   with `401` + `WWW-Authenticate`. This restores the original behaviour
///   exactly and keeps the standard client-driven OAuth flow working (a client
///   that receives the 401 launches its OAuth flow and retries with a token).
/// - [`AuthMode::Optional`] (`/agent` endpoint): token-less requests pass
///   through with no `BearerToken` but tagged with [`AgentEndpoint`], letting
///   the handshake succeed and the `authenticate` tool be listed/called.
///
/// When `restricted` is `Some` (a `/v2` public endpoint) a
/// [`RestrictedEndpoint`] marker carrying that [`RestrictedVersion`] is attached
/// to every request that proceeds, so handlers expose and accept only that
/// version's allowlist.
pub async fn mcp_auth_layer(
    mut req: Request,
    next: Next,
    base_url: &str,
    mode: AuthMode,
    restricted: Option<RestrictedVersion>,
) -> Response {
    let resource = crate::auth::metadata::resource_url_from_headers(req.headers(), base_url);
    // A restricted endpoint points at its own RFC 9728 resource-specific
    // metadata, which advertises read-only scopes only — so the authorize URL the
    // client builds never requests trading scopes.
    let metadata_path = match restricted {
        Some(version) => version.metadata_path(),
        None => "/.well-known/oauth-protected-resource",
    };
    let www_authenticate = format!("Bearer resource_metadata=\"{resource}{metadata_path}\"");

    let bearer_token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| t.to_string());

    match bearer_token {
        Some(token) => {
            req.extensions_mut().insert(BearerToken(token));
        }
        None => match mode {
            AuthMode::Required => {
                // Main endpoint: no credentials -> 401, exactly as before. This
                // is what drives standard MCP clients to start their OAuth flow.
                return (
                    StatusCode::UNAUTHORIZED,
                    [("WWW-Authenticate", www_authenticate.as_str())],
                    "missing or invalid Authorization header",
                )
                    .into_response();
            }
            AuthMode::Optional => {
                // `/agent` endpoint: let the request proceed so the handshake
                // and the `authenticate` tool work. Tag it so downstream
                // handlers know to expose only `authenticate`.
                req.extensions_mut().insert(AgentEndpoint);
            }
        },
    }

    // Tag the request so list_tools/call_tool restrict to the public allowlist.
    // Inserted regardless of token presence: restricted endpoints are
    // `AuthMode::Required`, so a token-less request has already been rejected
    // with 401 above.
    if let Some(version) = restricted {
        req.extensions_mut().insert(RestrictedEndpoint(version));
    }

    next.run(req).await
}
