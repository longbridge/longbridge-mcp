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
    let resource = crate::auth::metadata::public_url_from_headers(req.headers(), base_url).url;
    // A restricted endpoint points at its own RFC 9728 resource-specific
    // metadata so clients bind the OAuth flow to the endpoint resource URL.
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
                //
                // This 401 is otherwise invisible in our own logs: it's
                // rejected here, before any `#[tool]` method (and its
                // `measured_tool_call` logging) ever runs. A client-observed
                // "error" with no matching server-side log line is the usual
                // symptom of a request never getting this far — logging it
                // here closes that gap. No token to log in this branch (that's
                // exactly why the request was rejected).
                let user_agent = req
                    .headers()
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                tracing::warn!(
                    path = req.uri().path(),
                    restricted = restricted.map(|v| format!("{v:?}")),
                    user_agent,
                    "rejected request: missing or invalid Authorization header"
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SharedBuffer;
    use axum::Router;
    use axum::routing::get;
    use tower::ServiceExt;

    fn router() -> Router {
        Router::new().route(
            "/mcp",
            get(|| async { "ok" }).layer(axum::middleware::from_fn(
                move |req: Request, next: Next| async move {
                    mcp_auth_layer(req, next, "https://example.com", AuthMode::Required, None).await
                },
            )),
        )
    }

    #[tokio::test]
    async fn missing_token_is_rejected_with_401() {
        // Hits the same rejection-warn callsite as
        // `missing_token_rejection_is_logged`. Without the shared lock it could
        // register that callsite's interest (against no subscriber → disabled)
        // while the capturing test is mid-flight, swallowing the asserted line.
        let _lock = crate::test_support::LOG_CAPTURE_LOCK.lock().await;
        let response = router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/mcp")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            response.headers().contains_key("www-authenticate"),
            "401 must carry WWW-Authenticate per RFC 9728"
        );
    }

    /// The 401 above is otherwise silent — logging the rejection is the whole
    /// point of this change, so assert the log line actually fires rather
    /// than just checking the response.
    #[tokio::test]
    async fn missing_token_rejection_is_logged() {
        // Serialize with the other log-capturing tests (in `tools`) so their
        // `rebuild_interest_cache()` calls can't race this one and drop the
        // asserted line — see `LOG_CAPTURE_LOCK`'s docs.
        let _lock = crate::test_support::LOG_CAPTURE_LOCK.lock().await;
        let buffer = SharedBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // Same rationale as `measured_tool_call`'s tests: force a fresh
        // interest computation against the subscriber just installed, since
        // `tracing`'s per-callsite interest cache is process-global and
        // another test may have already cached this callsite as disabled.
        tracing::callsite::rebuild_interest_cache();

        router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/mcp")
                    .header("user-agent", "test-client/1.0")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let logged = buffer.contents();
        assert!(
            logged.contains("rejected request: missing or invalid Authorization header"),
            "expected the rejection to be logged: {logged}"
        );
        assert!(
            logged.contains("/mcp"),
            "expected the path in the log: {logged}"
        );
        assert!(
            logged.contains("test-client/1.0"),
            "expected the User-Agent in the log: {logged}"
        );
    }
}
