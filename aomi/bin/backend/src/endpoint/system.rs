use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::info;

use aomi_backend::{get_backend_request, ChatMessage, MessageSender, NamespaceAuth, Selection, SessionManager};
use aomi_core::SystemEvent;

use crate::auth::SessionId;

use super::types::SystemResponse;

type SharedSessionManager = Arc<SessionManager>;

/// Returns the first N words of a string for logging preview
fn first_n_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

#[derive(Serialize)]
struct MemoryModeResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

async fn updates_endpoint(
    // Alice: only talk to 1 SSE endpoint -> [title changed, tool complete, etc.]
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    info!(session_id, "GET /api/updates (SSE)");

    // Allow subscribing even if session doesn't exist yet - will filter by session_id
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx)
        .filter_map(move |result| {
            let (event_session_id, value) = result.ok()?;
            if event_session_id != session_id {
                return None;
            }
            Event::default().json_data(&value).ok()
        })
        .map(Ok::<_, Infallible>);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SystemResponse>, StatusCode> {
    let message = match params.get("message").cloned() {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let preview = first_n_words(&message, 3);
    info!(session_id, preview, "POST /api/system");

    let requested_backend = get_backend_request(&message);
    let requested_namespace = requested_backend.map(|ns| ns.as_str());

    // Use default namespace for system messages (no authorization required)
    let mut auth = NamespaceAuth::new(None, None, requested_namespace);

    let session_state = match session_manager
        .get_or_create_session(&session_id, &mut auth, Selection::default())
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    let res = state.send_ui_event(message).await.unwrap_or_else(|e| {
        ChatMessage::new(MessageSender::System, e.to_string(), Some("System Error"))
    });

    Ok(Json(SystemResponse { res }))
}

/// Get historical SSE events for a session.
///
/// Query params:
/// - `count` (optional): Number of recent events to return. If omitted, returns all.
///
/// Session ID is extracted from X-Session-Id header by auth middleware.
async fn get_events_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<SystemEvent>>, StatusCode> {
    info!(session_id, "GET /api/events");

    let count = params.get("count").and_then(|s| s.parse::<usize>().ok());

    let session_state = session_manager
        .get_session_if_exists(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let events = {
        let state = session_state.lock().await;
        state.get_sse_events(count)
    };

    Ok(Json(events))
}

async fn memory_mode_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<MemoryModeResponse>, StatusCode> {
    let memory_mode = params
        .get("memory_mode")
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false);

    info!(session_id, memory_mode, "POST /api/memory-mode");

    session_manager
        .set_memory_mode(&session_id, memory_mode)
        .await;

    Ok(Json(MemoryModeResponse {
        success: true,
        message: format!(
            "Memory mode {} for session",
            if memory_mode { "enabled" } else { "disabled" }
        ),
        data: Some(serde_json::json!({
            "session_id": session_id,
            "memory_mode": memory_mode
        })),
    }))
}

pub fn create_system_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/updates", get(updates_endpoint))
        .route("/events", get(get_events_endpoint))
        .route("/system", post(system_message_endpoint))
        .route("/memory-mode", post(memory_mode_endpoint))
}
