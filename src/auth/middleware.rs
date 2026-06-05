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
pub async fn mcp_auth_layer(
    mut req: Request,
    next: Next,
    base_url: &str,
    mode: AuthMode,
) -> Response {
    let resource = crate::auth::metadata::resource_url_from_headers(req.headers(), base_url);
    let www_authenticate =
        format!("Bearer resource_metadata=\"{resource}/.well-known/oauth-protected-resource\"");

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

    next.run(req).await
}
