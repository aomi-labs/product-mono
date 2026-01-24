use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{delete, get},
    Extension, Router,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

use aomi_backend::{ChatMessage, SessionManager};
use crate::auth::SessionId;

type SharedSessionManager = Arc<SessionManager>;

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
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<DbSessionInspection>, StatusCode> {
    info!(session_id, "GET /api/db/sessions/:id");
    let history_backend = session_manager.get_history_backend();
    let stored = match history_backend.get_session(&session_id).await {
        Ok(Some(data)) => data,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let message_count = stored.messages.len();
    Ok(Json(DbSessionInspection {
        session_id,
        title: Some(stored.title),
        messages: stored.messages,
        is_processing: false,
        message_count,
    }))
}

async fn db_messages_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<Vec<ChatMessage>>, StatusCode> {
    info!(session_id, "GET /api/db/sessions/:id/messages");
    let history_backend = session_manager.get_history_backend();
    let stored = match history_backend.get_session(&session_id).await {
        Ok(Some(data)) => data,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    Ok(Json(stored.messages))
}

async fn db_stats_endpoint(
    State(_session_manager): State<SharedSessionManager>,
) -> Result<Json<DbStats>, StatusCode> {
    info!("GET /api/db/stats");
    // Simple placeholder - can be extended later
    Ok(Json(DbStats { session_count: 0 }))
}

async fn db_cleanup_session_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<CleanupResponse>, StatusCode> {
    info!(session_id, "DELETE /api/db/sessions/:id/cleanup");
    // Delete from in-memory cache
    session_manager.delete_session(&session_id).await;

    // Delete from persistent storage
    let history_backend = session_manager.get_history_backend();
    if history_backend.delete_session(&session_id).await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(CleanupResponse {
        success: true,
        message: format!("Session {} deleted successfully", session_id),
    }))
}

async fn db_cleanup_all_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Json<CleanupAllResponse>, StatusCode> {
    info!("DELETE /api/db/cleanup-all");
    // Count sessions before cleanup
    let session_count = session_manager.active_session_count();

    // Delete all sessions (in-memory and persistent storage)
    session_manager.cleanup_all_sessions().await;

    Ok(Json(CleanupAllResponse {
        success: true,
        message: "All sessions cleaned up successfully".to_string(),
        sessions_deleted: session_count,
    }))
}

pub fn create_db_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/sessions/:session_id", get(db_session_endpoint))
        .route("/sessions/:session_id/messages", get(db_messages_endpoint))
        .route(
            "/sessions/:session_id/cleanup",
            delete(db_cleanup_session_endpoint),
        )
        .route("/cleanup-all", delete(db_cleanup_all_endpoint))
        .route("/stats", get(db_stats_endpoint))
}
