use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Redirect, Response};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::AppState;
use crate::auth::token;

/// Pending OAuth authorization state
struct PendingAuth {
    /// Dynamically registered Longbridge client_id for this user
    lb_client_id: String,
    /// MCP Client redirect_uri
    client_redirect_uri: String,
    /// PKCE code_challenge
    code_challenge: String,
    code_challenge_method: String,
    /// MCP Client state
    client_state: Option<String>,
}

/// Issued authorization code mapped to user info
struct IssuedCode {
    user_id: String,
    code_challenge: String,
    code_challenge_method: String,
    client_redirect_uri: String,
}

pub struct OAuthState {
    /// state_token -> PendingAuth
    pending: RwLock<HashMap<String, PendingAuth>>,
    /// authorization_code -> IssuedCode
    codes: RwLock<HashMap<String, IssuedCode>>,
}

impl OAuthState {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            codes: RwLock::new(HashMap::new()),
        }
    }
}

#[derive(Serialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
) -> Json<ProtectedResourceMetadata> {
    // TODO: derive base URL from request
    let base_url = state.base_url.clone();
    Json(ProtectedResourceMetadata {
        resource: base_url.clone(),
        authorization_servers: vec![base_url],
        scopes_supported: vec!["openapi".to_string()],
    })
}

#[derive(Serialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    response_types_supported: Vec<String>,
    grant_types_supported: Vec<String>,
    code_challenge_methods_supported: Vec<String>,
    scopes_supported: Vec<String>,
}

async fn authorization_server_metadata(
    State(state): State<Arc<AppState>>,
) -> Json<AuthorizationServerMetadata> {
    let base_url = state.base_url.clone();
    Json(AuthorizationServerMetadata {
        issuer: base_url.clone(),
        authorization_endpoint: format!("{base_url}/oauth/authorize"),
        token_endpoint: format!("{base_url}/oauth/token"),
        registration_endpoint: format!("{base_url}/oauth/register"),
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ],
        code_challenge_methods_supported: vec!["S256".to_string()],
        scopes_supported: vec!["openapi".to_string()],
    })
}

#[derive(Deserialize)]
pub struct AuthorizeParams {
    response_type: String,
    redirect_uri: String,
    code_challenge: String,
    code_challenge_method: String,
    state: Option<String>,
    /// Standard OAuth scope (accepted but not enforced yet)
    #[serde(default)]
    scope: Option<String>,
    /// Standard OAuth client_id (accepted but not enforced yet)
    #[serde(default)]
    client_id: Option<String>,
}

async fn authorize(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    if params.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "unsupported response_type").into_response();
    }
    if params.code_challenge_method != "S256" {
        return (StatusCode::BAD_REQUEST, "unsupported code_challenge_method").into_response();
    }

    tracing::debug!(
        scope = ?params.scope,
        client_id = ?params.client_id,
        "authorize request"
    );

    let callback_url = format!("{}/oauth/callback", state.base_url);
    let lb_client_id = match crate::auth::longbridge::register_client(&callback_url).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "failed to register Longbridge OAuth client");
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    let our_state = Uuid::new_v4().to_string();

    let oauth_state = state.registry.oauth_state();

    oauth_state.pending.write().await.insert(
        our_state.clone(),
        PendingAuth {
            lb_client_id: lb_client_id.clone(),
            client_redirect_uri: params.redirect_uri,
            code_challenge: params.code_challenge,
            code_challenge_method: params.code_challenge_method,
            client_state: params.state,
        },
    );

    // Redirect to Longbridge OAuth authorize
    let lb_api_url = std::env::var("LONGBRIDGE_HTTP_URL")
        .unwrap_or_else(|_| "https://openapi.longbridge.com".to_string());
    let lb_authorize_url = format!(
        "{lb_api_url}/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&state={}",
        lb_client_id,
        urlencoding::encode(&callback_url),
        our_state,
    );

    Redirect::temporary(&lb_authorize_url).into_response()
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let oauth_state = state.registry.oauth_state();

    let pending = {
        let mut pending_map = oauth_state.pending.write().await;
        match pending_map.remove(&params.state) {
            Some(p) => p,
            None => return (StatusCode::BAD_REQUEST, "invalid state").into_response(),
        }
    };

    // Exchange code for Longbridge tokens
    let callback_url = format!("{}/oauth/callback", state.base_url);
    let tokens = match crate::auth::longbridge::exchange_token(
        &pending.lb_client_id,
        &params.code,
        &callback_url,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to exchange Longbridge token");
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    // Save token file before loading via OAuthBuilder (it reads from disk)
    if let Err(e) = crate::auth::longbridge::save_token_file(&pending.lb_client_id, &tokens) {
        tracing::error!(error = %e, "failed to save token file");
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    // Create Config + HttpClient from the saved token
    let (config, http_client) =
        match crate::auth::longbridge::create_session(&pending.lb_client_id).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(error = %e, "failed to create Longbridge session");
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };

    let user_id = Uuid::new_v4().to_string();

    // Register user in DB and insert in-memory session
    if let Err(e) = state
        .registry
        .create_session(&user_id, &pending.lb_client_id, config, http_client)
        .await
    {
        tracing::error!(error = %e, "failed to create user session");
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    crate::metrics::inc_oauth_authorizations();

    // Issue authorization code for MCP client
    let auth_code = Uuid::new_v4().to_string();
    oauth_state.codes.write().await.insert(
        auth_code.clone(),
        IssuedCode {
            user_id,
            code_challenge: pending.code_challenge,
            code_challenge_method: pending.code_challenge_method,
            client_redirect_uri: pending.client_redirect_uri.clone(),
        },
    );

    // Redirect back to MCP client
    let mut redirect_url = pending.client_redirect_uri;
    redirect_url.push_str(&format!("?code={auth_code}"));
    if let Some(client_state) = pending.client_state {
        redirect_url.push_str(&format!("&state={client_state}"));
    }

    Redirect::temporary(&redirect_url).into_response()
}

#[derive(Deserialize)]
pub struct TokenRequest {
    grant_type: String,
    code: Option<String>,
    code_verifier: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: String,
}

async fn token_endpoint(
    State(state): State<Arc<AppState>>,
    axum::Form(params): axum::Form<TokenRequest>,
) -> Response {
    match params.grant_type.as_str() {
        "authorization_code" => {
            let code = match params.code {
                Some(c) => c,
                None => return (StatusCode::BAD_REQUEST, "missing code").into_response(),
            };
            let verifier = match params.code_verifier {
                Some(v) => v,
                None => return (StatusCode::BAD_REQUEST, "missing code_verifier").into_response(),
            };

            let oauth_state = state.registry.oauth_state();
            let issued = {
                let mut codes = oauth_state.codes.write().await;
                match codes.remove(&code) {
                    Some(c) => c,
                    None => return (StatusCode::BAD_REQUEST, "invalid code").into_response(),
                }
            };

            // Validate redirect_uri matches the one from the authorization request
            if let Some(ref redirect_uri) = params.redirect_uri
                && *redirect_uri != issued.client_redirect_uri
            {
                return (StatusCode::BAD_REQUEST, "redirect_uri mismatch").into_response();
            }

            // Verify PKCE
            if !verify_pkce(
                &verifier,
                &issued.code_challenge,
                &issued.code_challenge_method,
            ) {
                return (StatusCode::BAD_REQUEST, "PKCE verification failed").into_response();
            }

            let access = match token::issue_access_token(&state.jwt_secret, &issued.user_id) {
                Ok(t) => t,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };
            let refresh = match token::issue_refresh_token(&state.jwt_secret, &issued.user_id) {
                Ok(t) => t,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };

            Json(TokenResponse {
                access_token: access,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
                refresh_token: refresh,
            })
            .into_response()
        }
        "refresh_token" => {
            let refresh = match params.refresh_token {
                Some(r) => r,
                None => return (StatusCode::BAD_REQUEST, "missing refresh_token").into_response(),
            };

            let claims = match token::validate_token(&state.jwt_secret, &refresh, "refresh") {
                Ok(c) => c,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "invalid refresh_token").into_response();
                }
            };

            let access = match token::issue_access_token(&state.jwt_secret, &claims.sub) {
                Ok(t) => t,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };
            let new_refresh = match token::issue_refresh_token(&state.jwt_secret, &claims.sub) {
                Ok(t) => t,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
                }
            };

            Json(TokenResponse {
                access_token: access,
                token_type: "Bearer".to_string(),
                expires_in: 3600,
                refresh_token: new_refresh,
            })
            .into_response()
        }
        _ => (StatusCode::BAD_REQUEST, "unsupported grant_type").into_response(),
    }
}

fn verify_pkce(verifier: &str, challenge: &str, method: &str) -> bool {
    match method {
        "S256" => {
            use base64::Engine;
            use sha2::{Digest, Sha256};

            let mut hasher = Sha256::new();
            hasher.update(verifier.as_bytes());
            let hash = hasher.finalize();
            let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash);
            computed == challenge
        }
        "plain" => verifier == challenge,
        _ => false,
    }
}

pub async fn list_users(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let users = state.registry.list_users().await;
    Json(serde_json::json!({ "users": users }))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Response {
    match state.registry.revoke_user(&user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

/// RFC 7591 Dynamic Client Registration request.
#[derive(Debug, Deserialize)]
struct ClientRegistrationRequest {
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    grant_types: Option<Vec<String>>,
    #[serde(default)]
    response_types: Option<Vec<String>>,
}

/// RFC 7591 Dynamic Client Registration response.
#[derive(Serialize)]
struct ClientRegistrationResponse {
    client_id: String,
    client_name: Option<String>,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: String,
    grant_types: Vec<String>,
    response_types: Vec<String>,
}

/// MCP Client registers itself with our OAuth server (RFC 7591).
async fn register_client(
    Json(req): Json<ClientRegistrationRequest>,
) -> Json<ClientRegistrationResponse> {
    let client_id = Uuid::new_v4().to_string();
    tracing::info!(client_id, client_name = ?req.client_name, "registered MCP client");
    Json(ClientRegistrationResponse {
        client_id,
        client_name: req.client_name,
        redirect_uris: req.redirect_uris,
        token_endpoint_auth_method: req
            .token_endpoint_auth_method
            .unwrap_or_else(|| "none".to_string()),
        grant_types: req
            .grant_types
            .unwrap_or_else(|| vec!["authorization_code".to_string()]),
        response_types: req
            .response_types
            .unwrap_or_else(|| vec!["code".to_string()]),
    })
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            axum::routing::get(protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            axum::routing::get(authorization_server_metadata),
        )
        .route("/oauth/register", axum::routing::post(register_client))
        .route("/oauth/authorize", axum::routing::get(authorize))
        .route("/oauth/callback", axum::routing::get(callback))
        .route("/oauth/token", axum::routing::post(token_endpoint))
        .with_state(state)
}
