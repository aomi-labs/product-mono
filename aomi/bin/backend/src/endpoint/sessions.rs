use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::SessionId;
use aomi_backend::{NamespaceAuth, SessionManager};

type SharedSessionManager = Arc<SessionManager>;

async fn session_list_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let public_key = match params.get("public_key").cloned() {
        Some(pk) => pk,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    info!(public_key, "GET /api/sessions");

    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let sessions = session_manager
        .list_sessions(&public_key, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<_> = sessions
        .into_iter()
        .map(|s| {
            let is_archived = session_manager.is_session_archived(&s.session_id);
            json!({
                "session_id": s.session_id,
                "title": s.title,
                "is_archived": is_archived,
            })
        })
        .collect();
    Ok(Json(result))
}

async fn session_create_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let public_key = payload.get("public_key").cloned();
    info!(session_id, "POST /api/sessions (create)");

    // Use default namespace for session creation
    let mut auth = NamespaceAuth::new(public_key, None, None);

    let _session_state = session_manager
        .get_or_create_session(&session_id, &mut auth)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let title = session_manager.get_session_title(&session_id);

    Ok(Json(json!({
        "session_id": session_id,
        "title": title,
    })))
}

async fn session_get_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!(session_id, "GET /api/sessions/:id");

    if session_manager.get_session_if_exists(&session_id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let metadata = session_manager.get_session_metadata(&session_id);
    let (title, is_archived) = match metadata {
        Some(m) => (m.title, m.is_archived),
        None => (String::new(), false),
    };

    Ok(Json(json!({
        "session_id": session_id,
        "title": title,
        "is_archived": is_archived,
    })))
}

async fn session_delete_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<StatusCode, StatusCode> {
    info!(session_id, "DELETE /api/sessions/:id");
    session_manager.delete_session(&session_id).await;
    Ok(StatusCode::OK)
}

async fn session_rename_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<StatusCode, StatusCode> {
    let title = match payload.get("title").cloned() {
        Some(t) => t,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    info!(session_id, title, "PATCH /api/sessions/:id (rename)");

    session_manager
        .update_session_title(&session_id, title)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

async fn session_archive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<StatusCode, StatusCode> {
    info!(session_id, "POST /api/sessions/:id/archive");
    session_manager.set_session_archived(&session_id, true);
    Ok(StatusCode::OK)
}

async fn session_unarchive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<StatusCode, StatusCode> {
    info!(session_id, "POST /api/sessions/:id/unarchive");
    session_manager.set_session_archived(&session_id, false);
    Ok(StatusCode::OK)
}

pub fn create_sessions_router() -> Router<SharedSessionManager> {
    Router::new()
        .route(
            "/",
            get(session_list_endpoint).post(session_create_endpoint),
        )
        .route(
            "/:session_id",
            get(session_get_endpoint)
                .delete(session_delete_endpoint)
                .patch(session_rename_endpoint),
        )
        .route("/:session_id/archive", post(session_archive_endpoint))
        .route("/:session_id/unarchive", post(session_unarchive_endpoint))
}
