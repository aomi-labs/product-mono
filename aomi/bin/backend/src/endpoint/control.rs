use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Extension, Router,
};
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::SessionId;
use aomi_backend::{AuthorizedKey, NamespaceAuth, SessionManager};

pub type SharedSessionManager = Arc<SessionManager>;

/// HTTP endpoint to get allowed namespaces for the current request context.
///
/// Returns namespaces based on priority:
/// 1. API key namespaces (if provided and non-empty)
/// 2. User namespaces from database (if public_key provided)
/// 3. Default namespaces
pub async fn get_namespaces_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    api_key: Option<Extension<AuthorizedKey>>,
    params: Option<Query<HashMap<String, String>>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    info!(session_id, "GET /api/control/namespaces");

    // Get public key: first from query param, then from session cache
    let public_key = params
        .as_ref()
        .and_then(|p| p.get("public_key").cloned())
        .or_else(|| session_manager.get_public_key(&session_id));

    // Create NamespaceAuth and merge authorization
    let mut auth = NamespaceAuth::new(public_key.clone(), api_key.map(|e| e.0), None);

    // Merge authorization from API key and user namespaces
    let user_namespaces = if let Some(ref pk) = public_key {
        session_manager.get_user_namespaces(pk).await.ok()
    } else {
        None
    };
    auth.merge_authorization(user_namespaces);

    Ok(Json(auth.current_authorization))
}

pub fn create_control_router() -> Router<SharedSessionManager> {
    Router::new().route("/namespaces", get(get_namespaces_endpoint))
}
