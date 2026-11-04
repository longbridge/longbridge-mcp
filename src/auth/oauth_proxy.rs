//! Minimal OAuth authorize redirect plus token and revocation proxy.
//!
//! Dynamic registration and browser callbacks stay directly between the MCP
//! client and Longbridge. The authorization endpoint is a same-origin redirect
//! so clients that enforce issuer binding (e.g. Codex) accept the metadata.
//! Only credential-bearing POSTs are proxied so the server can derive and
//! attach `x-dc-region`.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use longbridge::DcRegion;

use crate::auth::AppState;
use crate::auth::metadata::{oauth_upstream_url, public_url_from_headers};
use crate::metrics::{record_oauth_authorize, record_oauth_revoke, record_oauth_token};

static OAUTH_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("longbridge-mcp/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build upstream OAuth HTTP client")
});

const TOKEN_CREDENTIALS: &[&str] = &["refresh_token", "code"];
const REVOKE_CREDENTIALS: &[&str] = &["token"];

/// 正在代理的是哪种携带凭据的 OAuth POST,决定 `forward` 记哪个指标。
#[derive(Clone, Copy)]
enum OAuthOp {
    Token,
    Revoke,
}

/// 把 form 里的 `grant_type` 归到有界桶,作为低基数 label。
fn grant_type_of(pairs: &[(String, String)]) -> &'static str {
    match pairs
        .iter()
        .find(|(key, _)| key == "grant_type")
        .map(|(_, value)| value.as_str())
    {
        Some("authorization_code") => "authorization_code",
        Some("refresh_token") => "refresh_token",
        _ => "other",
    }
}

/// 把上游是否 2xx 映射为指标 `result` label。
fn result_label(is_success: bool) -> &'static str {
    if is_success { "success" } else { "error" }
}

/// 按操作类型记录对应的 OAuth 指标。
fn record(op: OAuthOp, grant_type: &str, result: &str) {
    match op {
        OAuthOp::Token => record_oauth_token(grant_type, result),
        OAuthOp::Revoke => record_oauth_revoke(result),
    }
}

/// Redirect the browser to the upstream authorization endpoint.
///
/// The advertised `authorization_endpoint` must share the issuer's origin:
/// clients such as Codex reject cross-origin authorization endpoints unless
/// the server supports RFC 9207 issuer-bound callbacks. The query string is
/// forwarded verbatim so PKCE, `state`, and `redirect_uri` reach Longbridge
/// unchanged.
pub async fn authorize(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let public = public_url_from_headers(&headers, &state.base_url);
    let upstream = oauth_upstream_url(public.via_global_entry);
    record_oauth_authorize();
    // Each redirect carries per-login `state` and PKCE values; never let an
    // edge cache replay one user's redirect to another.
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Redirect::to(&authorize_redirect_url(&upstream, query.as_deref())),
    )
        .into_response()
}

/// Join an upstream OAuth base URL and a leading-slash endpoint path.
fn upstream_endpoint(upstream: &str, path: &str) -> String {
    format!("{}{path}", upstream.trim_end_matches('/'))
}

fn authorize_redirect_url(upstream: &str, query: Option<&str>) -> String {
    let base = upstream_endpoint(upstream, "/oauth2/authorize");
    match query.filter(|query| !query.is_empty()) {
        Some(query) => format!("{base}?{query}"),
        None => base,
    }
}

/// Proxy a token exchange and attach the region derived from its credential.
pub async fn token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        &state,
        &headers,
        "/oauth2/token",
        TOKEN_CREDENTIALS,
        OAuthOp::Token,
        body,
    )
    .await
}

/// Proxy token revocation and attach the region derived from the token.
pub async fn revoke(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        &state,
        &headers,
        "/oauth2/revoke",
        REVOKE_CREDENTIALS,
        OAuthOp::Revoke,
        body,
    )
    .await
}

fn derive_region(pairs: &[(String, String)], credential_fields: &[&str]) -> DcRegion {
    credential_fields
        .iter()
        .find_map(|name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| DcRegion::from_credential(value))
        })
        .unwrap_or(DcRegion::Ap)
}

async fn forward(
    state: &AppState,
    headers: &HeaderMap,
    upstream_path: &str,
    credential_fields: &[&str],
    op: OAuthOp,
    body: Bytes,
) -> Response {
    let pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body).unwrap_or_default();
    let region = derive_region(&pairs, credential_fields);
    let grant_type = grant_type_of(&pairs);
    let public = public_url_from_headers(headers, &state.base_url);
    let upstream = oauth_upstream_url(public.via_global_entry);
    let url = upstream_endpoint(&upstream, upstream_path);

    match upstream_request(&url, region, body).send().await {
        Ok(response) => {
            record(op, grant_type, result_label(response.status().is_success()));
            relay(response).await
        }
        Err(error) => {
            record(op, grant_type, "error");
            (
                StatusCode::BAD_GATEWAY,
                format!("upstream OAuth request failed: {error}"),
            )
                .into_response()
        }
    }
}

fn upstream_request(url: &str, region: DcRegion, body: Bytes) -> reqwest::RequestBuilder {
    OAUTH_HTTP_CLIENT
        .post(url)
        .header(
            header::CONTENT_TYPE.as_str(),
            "application/x-www-form-urlencoded",
        )
        .header(longbridge::DC_REGION_HEADER, region.as_str())
        .body(body)
}

async fn relay(upstream: reqwest::Response) -> Response {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let body = upstream.bytes().await.unwrap_or_default();
    (
        status,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::PRAGMA, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn authorize_redirect_preserves_query_verbatim() {
        let query = "response_type=code&client_id=abc&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fcallback&code_challenge=x&code_challenge_method=S256&state=s%2B1";
        assert_eq!(
            authorize_redirect_url("https://openapi.longbridge.com/", Some(query)),
            format!("https://openapi.longbridge.com/oauth2/authorize?{query}")
        );
    }

    #[test]
    fn authorize_redirect_without_query() {
        assert_eq!(
            authorize_redirect_url("https://openapi.longbridge.com", None),
            "https://openapi.longbridge.com/oauth2/authorize"
        );
        assert_eq!(
            authorize_redirect_url("https://openapi.longbridge.com", Some("")),
            "https://openapi.longbridge.com/oauth2/authorize"
        );
    }

    #[test]
    fn grant_type_buckets() {
        assert_eq!(
            grant_type_of(&pairs(&[("grant_type", "authorization_code")])),
            "authorization_code"
        );
        assert_eq!(
            grant_type_of(&pairs(&[("grant_type", "refresh_token")])),
            "refresh_token"
        );
        assert_eq!(
            grant_type_of(&pairs(&[("grant_type", "client_credentials")])),
            "other"
        );
        assert_eq!(grant_type_of(&pairs(&[("code", "x")])), "other");
    }

    #[test]
    fn result_label_maps_status() {
        assert_eq!(result_label(true), "success");
        assert_eq!(result_label(false), "error");
    }

    #[test]
    fn token_region_uses_refresh_token_before_code() {
        let form = pairs(&[("code", "ap_code"), ("refresh_token", "us_refresh")]);
        assert_eq!(derive_region(&form, TOKEN_CREDENTIALS), DcRegion::Us);
    }

    #[test]
    fn token_region_uses_authorization_code() {
        let form = pairs(&[("code", "us_code")]);
        assert_eq!(derive_region(&form, TOKEN_CREDENTIALS), DcRegion::Us);
    }

    #[test]
    fn revoke_region_uses_token() {
        let form = pairs(&[("token", "us_access_token")]);
        assert_eq!(derive_region(&form, REVOKE_CREDENTIALS), DcRegion::Us);
    }

    #[test]
    fn unrecognized_or_missing_credential_defaults_to_ap() {
        assert_eq!(
            derive_region(&pairs(&[("code", "opaque")]), TOKEN_CREDENTIALS),
            DcRegion::Ap
        );
        assert_eq!(derive_region(&[], TOKEN_CREDENTIALS), DcRegion::Ap);
    }

    #[test]
    fn upstream_request_adds_region_without_rewriting_form_body() {
        let body = Bytes::from_static(
            b"grant_type=authorization_code&code=us_code&redirect_uri=http%3A%2F%2F127.0.0.1%3A1234%2Fcallback",
        );
        let request = upstream_request(
            "https://openapi.longbridge.com/oauth2/token",
            DcRegion::Us,
            body.clone(),
        )
        .build()
        .expect("OAuth proxy request should build");

        assert_eq!(
            request
                .headers()
                .get(longbridge::DC_REGION_HEADER)
                .expect("DC region header should be present"),
            "us"
        );
        assert_eq!(
            request.body().and_then(reqwest::Body::as_bytes),
            Some(body.as_ref())
        );
    }

    #[test]
    fn malformed_form_defaults_to_ap_without_changing_bytes() {
        let body = Bytes::from_static(b"code=%GG&redirect_uri=http://127.0.0.1/callback");
        let region = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body)
            .map(|pairs| derive_region(&pairs, TOKEN_CREDENTIALS))
            .unwrap_or(DcRegion::Ap);
        let request = upstream_request(
            "https://openapi.longbridge.com/oauth2/token",
            region,
            body.clone(),
        )
        .build()
        .expect("OAuth proxy request should build");

        assert_eq!(
            request
                .headers()
                .get(longbridge::DC_REGION_HEADER)
                .expect("DC region header should be present"),
            "ap"
        );
        assert_eq!(
            request.body().and_then(reqwest::Body::as_bytes),
            Some(body.as_ref())
        );
    }
}
