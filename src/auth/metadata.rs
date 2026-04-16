use std::sync::Arc;

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;

use crate::auth::AppState;

#[derive(Serialize)]
pub(crate) struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

pub async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
) -> Json<ProtectedResourceMetadata> {
    let base_url = state.base_url.clone();
    Json(ProtectedResourceMetadata {
        resource: base_url.clone(),
        authorization_servers: vec![base_url],
        scopes_supported: vec!["openapi".to_string()],
    })
}

#[derive(Serialize)]
pub(crate) struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    response_types_supported: Vec<String>,
    grant_types_supported: Vec<String>,
    code_challenge_methods_supported: Vec<String>,
    scopes_supported: Vec<String>,
}

pub async fn authorization_server_metadata(
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
