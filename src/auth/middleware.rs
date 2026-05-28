use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Bearer token extracted from the Authorization header.
#[derive(Clone, Debug)]
pub struct BearerToken(pub String);

/// Auth middleware for MCP endpoints.
///
/// Extracts Bearer token from Authorization header and stores it as
/// `BearerToken` in request extensions. No JWT validation -- the token
/// is forwarded to Longbridge SDK calls directly.
///
/// On 401 responses, includes `resource_metadata` in the `WWW-Authenticate`
/// header as required by the MCP OAuth 2.1 spec (RFC 9728).
pub async fn mcp_auth_layer(mut req: Request, next: Next, base_url: &str) -> Response {
    let resource = crate::auth::metadata::resource_url_from_headers(req.headers(), base_url);
    let www_authenticate =
        format!("Bearer resource_metadata=\"{resource}/.well-known/oauth-protected-resource\"");

    let bearer_token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| t.to_string());

    let bearer_token = match bearer_token {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                [("WWW-Authenticate", www_authenticate.as_str())],
                "missing or invalid Authorization header",
            )
                .into_response();
        }
    };

    req.extensions_mut().insert(BearerToken(bearer_token));

    next.run(req).await
}
