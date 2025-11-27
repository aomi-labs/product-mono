use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

use aomi_backend::{
    generate_session_id, session::{HistorySession, FullSessionState}, SessionManager,
};

type SharedSessionManager = Arc<SessionManager>;

async fn session_list_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<HistorySession>>, StatusCode> {
    let public_key = match params.get("public_key").cloned() {
        Some(pk) => pk,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    session_manager
        .get_history_sessions(&public_key, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn session_create_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session_id = generate_session_id();
    let public_key = payload.get("public_key").cloned();

    // Get title from frontend, or use truncated session_id as fallback
    let title = payload.get("title").cloned().or_else(|| {
        let mut placeholder = session_id.clone();
        placeholder.truncate(6);
        Some(placeholder)
    });

    session_manager
        .set_session_public_key(&session_id, public_key.clone())
        .await;

    let session_state = session_manager
        .get_or_create_session(&session_id, None, title.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get actual title from session state (might be None if creation failed)
    let final_title = {
        let state = session_state.lock().await;
        state.get_title().map(|s| s.to_string())
    };

    Ok(Json(json!({
        "session_id": session_id,
        "title": final_title.or(title),
    })))
}

async fn session_get_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<Json<FullSessionState>, StatusCode> {
    let pubkey = session_manager.get_public_key(&session_id);
    let session_state = match session_manager
        .get_or_create_session(&session_id, None, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };


    let mut state = session_state.lock().await;
    state.update_state().await;
    let mut full_state = state.get_full_state();
    full_state.pubkey = pubkey;
    Ok(Json(full_state))
}

async fn session_delete_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager.delete_session(&session_id).await;
    Ok(StatusCode::OK)
}

async fn session_rename_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<StatusCode, StatusCode> {
    let title = match payload.get("title").cloned() {
        Some(t) => t,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    session_manager
        .update_session_title(&session_id, title)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

async fn session_archive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager
        .set_session_archived(&session_id, true)
        .await;
    Ok(StatusCode::OK)
}

async fn session_unarchive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager
        .set_session_archived(&session_id, false)
        .await;
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
