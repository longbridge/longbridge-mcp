use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Treat a token as expired this many seconds before its `exp`, so a request
/// doesn't pass here and then expire on its way upstream.
const EXPIRY_SKEW_SECS: i64 = 30;

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
    /// `/public` — login-free market data for an upstream gateway. The
    /// gateway's Bearer token is forwarded to OpenAPI as-is (see
    /// [`public_auth_layer`]). Only the `PUBLIC` allowlist: market data,
    /// fundamentals, news and research, never anything account-bound or a
    /// write.
    Public,
}

impl RestrictedVersion {
    /// RFC 9728 protected-resource-metadata path advertised to clients that hit
    /// a `401` on this endpoint, so the authorize URL requests the matching
    /// read-only consent set.
    pub fn metadata_path(self) -> &'static str {
        match self {
            RestrictedVersion::V2 => "/.well-known/oauth-protected-resource/v2",
            // `/public` answers 401 without `resource_metadata`, so this is
            // never advertised; it falls back to the server-wide metadata to
            // stay well-formed.
            RestrictedVersion::Public => "/.well-known/oauth-protected-resource",
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

/// Set by `call_tool` on `/public` when OpenAPI rejects the forwarded token
/// (an upstream `401`), so [`public_auth_layer`] can answer with HTTP `401`
/// instead of a `200` carrying a tool error.
#[derive(Clone, Debug, Default)]
pub struct UpstreamUnauthorized(Arc<AtomicBool>);

impl UpstreamUnauthorized {
    /// Record that upstream rejected this request's token.
    pub fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// `401` for `/public`: an RFC 6750 `invalid_token` challenge with no
/// `resource_metadata`, since this endpoint has no OAuth flow of its own; the
/// gateway in front of it obtains a fresh token and retries.
fn public_unauthorized(description: &str) -> Response {
    let challenge = format!("Bearer error=\"invalid_token\", error_description=\"{description}\"");
    (
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", challenge.as_str())],
        description.to_string(),
    )
        .into_response()
}

/// Auth middleware for the login-free `/public` endpoint.
///
/// The caller (an upstream gateway) sends its own `Authorization: Bearer`
/// token on every request; it is passed upstream as-is, never parsed, stored
/// or logged, and may differ from one request to the next. A request without
/// one gets `401 invalid_token`. The request is tagged
/// [`RestrictedEndpoint`]`(Public)` so handlers expose and accept only the
/// public allowlist, and carries an [`UpstreamUnauthorized`] flag: when a tool
/// call's token is rejected upstream, the response is replaced with `401
/// invalid_token` too. That works because `/public` serves plain JSON
/// responses, so the call has finished by the time `next` returns.
pub async fn public_auth_layer(mut req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    let Some(token) = token else {
        tracing::info!(
            path = req.uri().path(),
            "public endpoint: rejected request without a bearer token"
        );
        return public_unauthorized("missing bearer token");
    };
    let unauthorized = UpstreamUnauthorized::default();
    let extensions = req.extensions_mut();
    extensions.insert(BearerToken(token));
    extensions.insert(RestrictedEndpoint(RestrictedVersion::Public));
    extensions.insert(unauthorized.clone());
    let response = next.run(req).await;
    if unauthorized.is_set() {
        return public_unauthorized("the access token was rejected upstream");
    }
    response
}

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
///
/// A JWT whose `exp` has passed (or passes within [`EXPIRY_SKEW_SECS`]) is
/// rejected as `invalid_token`; on the optional `/agent` endpoint it is instead
/// dropped and the request handled as unauthenticated.
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

    // An expired JWT would otherwise pass straight through and fail every
    // tool call upstream with `token is expired` inside an `isError` result —
    // which MCP clients never treat as a reason to refresh, so the user stays
    // stuck. Rejecting it here with RFC 6750 `invalid_token` makes OAuth
    // clients (Claude, ChatGPT) run their refresh-token flow instead.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let bearer_token = match bearer_token {
        Some(token) if crate::auth::jwt::is_expired_at(&token, now + EXPIRY_SKEW_SECS) => {
            let user_agent = req
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok());
            crate::metrics::record_auth_expired(crate::metrics::classify_client(user_agent));
            tracing::info!(
                path = req.uri().path(),
                user_agent = user_agent.unwrap_or(""),
                "rejected request: bearer token expired"
            );
            match mode {
                AuthMode::Required => {
                    let challenge = format!(
                        "{www_authenticate}, error=\"invalid_token\", error_description=\"The access token expired\""
                    );
                    return (
                        StatusCode::UNAUTHORIZED,
                        [("WWW-Authenticate", challenge.as_str())],
                        "access token expired",
                    )
                        .into_response();
                }
                // `/agent` clients may have no OAuth support at all; treat the
                // request as unauthenticated so they are offered `authenticate`
                // again rather than locked out by a 401.
                AuthMode::Optional => None,
            }
        }
        other => other,
    };

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

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    async fn call(mode: AuthMode, token: &str) -> (StatusCode, String, String) {
        let app = Router::new().route(
            "/mcp",
            get(|req: Request| async move {
                match req.extensions().get::<BearerToken>() {
                    Some(_) => "bearer",
                    None => "anonymous",
                }
            })
            .layer(axum::middleware::from_fn(
                move |req: Request, next: Next| async move {
                    mcp_auth_layer(req, next, "https://example.com", mode, None).await
                },
            )),
        );
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/mcp")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let challenge = response
            .headers()
            .get("www-authenticate")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        (status, challenge, String::from_utf8(body.to_vec()).unwrap())
    }

    async fn call_public(
        authorization: Option<&str>,
        reject_upstream: bool,
    ) -> (StatusCode, String, String) {
        let app = Router::new().route(
            "/public",
            get(move |req: Request| async move {
                let token = req
                    .extensions()
                    .get::<BearerToken>()
                    .map(|t| t.0.clone())
                    .unwrap_or_default();
                let restricted = req.extensions().get::<RestrictedEndpoint>().map(|r| r.0);
                if reject_upstream {
                    req.extensions()
                        .get::<UpstreamUnauthorized>()
                        .expect("public requests carry the upstream-401 flag")
                        .set();
                }
                format!("{token}|{restricted:?}")
            })
            .layer(axum::middleware::from_fn(public_auth_layer)),
        );
        let mut builder = axum::http::Request::builder().uri("/public");
        if let Some(value) = authorization {
            builder = builder.header("Authorization", value);
        }
        let response = app
            .oneshot(builder.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let challenge = response
            .headers()
            .get("www-authenticate")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        (status, challenge, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn public_endpoint_forwards_the_callers_token() {
        for (token, expected) in [("gw-token-1", "gw-token-1"), ("gw-token-2", "gw-token-2")] {
            let (status, challenge, body) =
                call_public(Some(&format!("Bearer {token}")), false).await;
            assert_eq!(status, StatusCode::OK);
            assert!(challenge.is_empty(), "{challenge}");
            assert_eq!(body, format!("{expected}|Some(Public)"));
        }
    }

    #[tokio::test]
    async fn public_endpoint_without_a_token_is_401_without_resource_metadata() {
        for authorization in [None, Some("Bearer "), Some("Basic abc")] {
            let (status, challenge, _) = call_public(authorization, false).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{authorization:?}");
            assert!(
                challenge.starts_with(r#"Bearer error="invalid_token""#),
                "{challenge}"
            );
            assert!(!challenge.contains("resource_metadata"), "{challenge}");
        }
    }

    #[tokio::test]
    async fn public_endpoint_turns_an_upstream_401_into_http_401() {
        let (status, challenge, _) = call_public(Some("Bearer gw-token"), true).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            challenge.starts_with(r#"Bearer error="invalid_token""#),
            "{challenge}"
        );
        assert!(!challenge.contains("resource_metadata"), "{challenge}");
    }

    #[tokio::test]
    async fn expired_jwt_is_rejected_as_invalid_token() {
        let expired = crate::auth::jwt::test_token(&format!(r#"{{"exp":{}}}"#, now() - 60));
        let (status, challenge, _) = call(AuthMode::Required, &expired).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            challenge.contains(r#"error="invalid_token""#),
            "{challenge}"
        );
        assert!(challenge.contains("resource_metadata="), "{challenge}");
    }

    #[tokio::test]
    async fn about_to_expire_jwt_is_rejected_within_the_skew() {
        let expiring = crate::auth::jwt::test_token(&format!(r#"{{"exp":{}}}"#, now() + 5));
        let (status, _, _) = call(AuthMode::Required, &expiring).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_or_opaque_tokens_pass_through() {
        let valid = crate::auth::jwt::test_token(&format!(r#"{{"exp":{}}}"#, now() + 3600));
        for token in [valid.as_str(), "opaque-token", "a.b.c"] {
            let (status, _, body) = call(AuthMode::Required, token).await;
            assert_eq!(
                (status, body.as_str()),
                (StatusCode::OK, "bearer"),
                "{token}"
            );
        }
    }

    #[tokio::test]
    async fn expired_jwt_on_agent_endpoint_is_downgraded_to_anonymous() {
        let expired = crate::auth::jwt::test_token(&format!(r#"{{"exp":{}}}"#, now() - 60));
        let (status, _, body) = call(AuthMode::Optional, &expired).await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, "anonymous"));
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
