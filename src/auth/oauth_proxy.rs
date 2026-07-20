//! Minimal OAuth token and revocation proxy.
//!
//! Authorization, dynamic registration, and browser callbacks stay directly
//! between the MCP client and Longbridge. Only credential-bearing POSTs pass
//! through this module so the server can derive and attach `x-dc-region`.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use longbridge::DcRegion;

use crate::auth::AppState;
use crate::auth::metadata::{oauth_upstream_url, public_url_from_headers};

static OAUTH_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("longbridge-mcp/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build upstream OAuth HTTP client")
});

const TOKEN_CREDENTIALS: &[&str] = &["refresh_token", "code"];
const REVOKE_CREDENTIALS: &[&str] = &["token"];

/// Proxy a token exchange and attach the region derived from its credential.
pub async fn token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(&state, &headers, "/oauth2/token", TOKEN_CREDENTIALS, body).await
}

/// Proxy token revocation and attach the region derived from the token.
pub async fn revoke(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(&state, &headers, "/oauth2/revoke", REVOKE_CREDENTIALS, body).await
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
    body: Bytes,
) -> Response {
    let region = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&body)
        .map(|pairs| derive_region(&pairs, credential_fields))
        .unwrap_or(DcRegion::Ap);
    let public = public_url_from_headers(headers, &state.base_url);
    let upstream = oauth_upstream_url(public.via_global_entry);
    let url = format!("{}{upstream_path}", upstream.trim_end_matches('/'));

    match upstream_request(&url, region, body).send().await {
        Ok(response) => relay(response).await,
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            format!("upstream OAuth request failed: {error}"),
        )
            .into_response(),
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
