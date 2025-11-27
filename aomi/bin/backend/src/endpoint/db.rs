use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get},
    Router,
};
use serde::Serialize;
use std::sync::Arc;

use aomi_backend::{ChatMessage, SessionManager};

type SharedSessionManager = Arc<SessionManager>;

// Check if cleanup operations are enabled
fn is_test_mode_enabled() -> bool {
    std::env::var("TEST_MODE")
        .ok()
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false)
}

#[derive(Serialize)]
pub struct DbSessionInspection {
    pub session_id: String,
    pub title: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub is_processing: bool,
    pub message_count: usize,
}

#[derive(Serialize)]
pub struct DbStats {
    pub session_count: usize,
}

#[derive(Serialize)]
pub struct CleanupResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct CleanupAllResponse {
    pub success: bool,
    pub message: String,
    pub sessions_deleted: usize,
}

async fn db_session_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<Json<DbSessionInspection>, StatusCode> {
    let session_state = match session_manager
        .get_or_create_session(&session_id, None, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let state = session_state.lock().await;
    let session_response = state.get_state();

    Ok(Json(DbSessionInspection {
        session_id,
        title: session_response.title,
        messages: session_response.messages.clone(),
        is_processing: session_response.is_processing,
        message_count: session_response.messages.len(),
    }))
}

async fn db_messages_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<ChatMessage>>, StatusCode> {
    let session_state = match session_manager
        .get_or_create_session(&session_id, None, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let state = session_state.lock().await;
    let session_response = state.get_state();

    Ok(Json(session_response.messages))
}

async fn db_stats_endpoint(
    State(_session_manager): State<SharedSessionManager>,
) -> Result<Json<DbStats>, StatusCode> {
    // Simple placeholder - can be extended later
    Ok(Json(DbStats {
        session_count: 0,
    }))
}

async fn db_cleanup_session_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<Json<CleanupResponse>, StatusCode> {
    if !is_test_mode_enabled() {
        return Err(StatusCode::FORBIDDEN);
    }

    session_manager.delete_session(&session_id).await;

    Ok(Json(CleanupResponse {
        success: true,
        message: format!("Session {} deleted successfully", session_id),
    }))
}

async fn db_cleanup_all_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Json<CleanupAllResponse>, StatusCode> {
    if !is_test_mode_enabled() {
        return Err(StatusCode::FORBIDDEN);
    }

    session_manager.cleanup_all_sessions().await;

    Ok(Json(CleanupAllResponse {
        success: true,
        message: "All sessions cleaned up successfully".to_string(),
        sessions_deleted: 0,
    }))
}

pub fn create_db_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/sessions/:session_id", get(db_session_endpoint))
        .route("/sessions/:session_id/messages", get(db_messages_endpoint))
        .route("/sessions/:session_id/cleanup", delete(db_cleanup_session_endpoint))
        .route("/cleanup-all", delete(db_cleanup_all_endpoint))
        .route("/stats", get(db_stats_endpoint))
}
