use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Json, Response};

use crate::auth::AppState;

pub async fn list_users(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let users = state.registry.list_users().await;
    Json(serde_json::json!({ "users": users }))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Response {
    use axum::response::IntoResponse;

    match state.registry.revoke_user(&user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}
