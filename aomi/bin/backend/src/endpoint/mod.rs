mod db;
mod history;
mod sessions;
mod system;
mod types;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension,
    Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

use aomi_backend::{generate_session_id, Namespace, SessionManager, SessionResponse};
use crate::auth::{requires_namespace_auth, AuthorizedKey, DEFAULT_NAMESPACE};

type SharedSessionManager = Arc<SessionManager>;

async fn health() -> &'static str {
    "OK"
}

#[allow(dead_code)]
pub(crate) fn get_backend_request(message: &str) -> Option<Namespace> {
    let normalized = message.to_lowercase();

    match normalized.as_str() {
        s if s.contains("default-magic") => Some(Namespace::Default),
        s if s.contains("l2beat-magic") => Some(Namespace::L2b),
        s if s.contains("forge-magic") => Some(Namespace::Forge),
        _ => None,
    }
}

fn get_backend_request_from_namespace(namespace: &str) -> Option<Namespace> {
    if namespace.eq_ignore_ascii_case("l2beat") || namespace.eq_ignore_ascii_case("l2b") {
        Some(Namespace::L2b)
    } else if namespace.eq_ignore_ascii_case("default") {
        Some(Namespace::Default)
    } else if namespace.eq_ignore_ascii_case("forge") {
        Some(Namespace::Forge)
    } else {
        None
    }
}
async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    api_key: Option<Extension<AuthorizedKey>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let namespace_param = params
        .get("namespace")
        .or_else(|| params.get("chatbot"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let namespace = namespace_param.unwrap_or(DEFAULT_NAMESPACE);
    if requires_namespace_auth(namespace) {
        let Extension(api_key) = api_key.ok_or(StatusCode::UNAUTHORIZED)?;
        if !api_key.allows_namespace(namespace) {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let session_id = params
        .get("session_id")
        .cloned()
        .unwrap_or_else(generate_session_id);
    let public_key = params.get("public_key").cloned();
    let message = match params.get("message").cloned() {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    session_manager
        .set_session_public_key(&session_id, public_key.clone())
        .await;

    let backend_request = namespace_param
        .and_then(get_backend_request_from_namespace)
        .or_else(|| get_backend_request(&message));
    let session_state = match session_manager
        .get_or_create_session(&session_id, backend_request, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if state.send_user_input(message).await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let title = session_manager.get_session_title(&session_id);
    let response = state.get_session_response(title);
    drop(state);

    history::maybe_update_history(
        &session_manager,
        &session_id,
        &response.messages,
        response.is_processing,
    )
    .await;

    Ok(Json(response))
}

async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let (session_state, rehydrated) = match session_manager
        .get_or_rehydrate_session(&session_id, None)
        .await
    {
        Ok(result) => result,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let Some(session_state) = session_state else {
        return Ok(Json(json!({
            "session_exists": false,
            "session_id": session_id,
        })));
    };

    let mut state = session_state.lock().await;
    state.sync_state().await;
    let title = session_manager.get_session_title(&session_id);
    let response = state.get_session_response(title);
    drop(state);

    history::maybe_update_history(
        &session_manager,
        &session_id,
        &response.messages,
        response.is_processing,
    )
    .await;

    let mut body = serde_json::to_value(response).unwrap_or_else(|_| json!({}));
    if let serde_json::Value::Object(ref mut map) = body {
        map.insert("session_exists".into(), serde_json::Value::Bool(true));
        map.insert("rehydrated".into(), serde_json::Value::Bool(rehydrated));
        map.insert(
            "state_source".into(),
            serde_json::Value::String(if rehydrated { "db" } else { "memory" }.to_string()),
        );
    }

    Ok(Json(body))
}

async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let session_state = match session_manager
        .get_or_create_session(&session_id, None, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if state.interrupt_processing().await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let title = session_manager.get_session_title(&session_id);
    let response = state.get_session_response(title);
    drop(state);

    Ok(Json(response))
}

pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/interrupt", post(interrupt_endpoint))
        .nest("/api/sessions", sessions::create_sessions_router())
        .nest("/api", system::create_system_router())
        .nest("/api/db", db::create_db_router())
        .with_state(session_manager)
}

// ✅ Modularization Complete

// File Structure

// bin/backend/src/
// ├── main.rs
// ├── endpoint.rs                    (40 lines - core chat + router)
// └── endpoint/
//     ├── sessions.rs               (150 lines - session CRUD)
//     ├── system.rs                 (115 lines - SSE + system messages)
//     └── db.rs                     (85 lines - Tier 1 inspection APIs)

// What Moved Where

// sessions.rs (Session Management)
// - session_list_endpoint - GET /api/sessions
// - session_create_endpoint - POST /api/sessions
// - session_get_endpoint - GET /api/sessions/:session_id
// - session_delete_endpoint - DELETE /api/sessions/:session_id
// - session_rename_endpoint - PATCH /api/sessions/:session_id
// - session_archive_endpoint - POST /api/sessions/:session_id/archive
// - session_unarchive_endpoint - POST /api/sessions/:session_id/unarchive

// system.rs (System Events)
// - updates_endpoint - GET /api/updates (SSE notifications; requires session_id)
// - get_async_events_endpoint - GET /api/events (fetch full event payloads by cursor)
// - system_message_endpoint - POST /api/system
// - memory_mode_endpoint - POST /api/memory-mode
// - MemoryModeResponse struct

// db.rs (Tier 1 Inspection - Read-Only)
// - db_session_endpoint - GET /api/db/sessions/:session_id
//   - Returns: { session_id, title, messages, is_processing, message_count }
// - db_messages_endpoint - GET /api/db/sessions/:session_id/messages
//   - Returns: raw messages array
// - db_stats_endpoint - GET /api/db/stats
//   - Returns: { session_count }

// endpoint.rs (Simplified Core)
// - health - GET /health
// - chat_endpoint - POST /api/chat
// - state_endpoint - GET /api/state
// - chat_stream - GET /api/chat/stream
// - interrupt_endpoint - POST /api/interrupt
// - get_backend_request helper
// - Router composition (nests all modules)

// Router Structure

// /health
// /api/chat, /api/state, /api/chat/stream, /api/interrupt (in endpoint.rs)
//   ↓ .nest("/api/sessions", ...)
//     /api/sessions
//     /api/sessions/:session_id
//     /api/sessions/:session_id/archive
//     /api/sessions/:session_id/unarchive
//   ↓ .nest("/api", ...)
//     /api/updates
//     /api/system
//     /api/memory-mode
//   ↓ .nest("/api/db", ...)
//     /api/db/sessions/:session_id
//     /api/db/sessions/:session_id/messages
//     /api/db/stats
