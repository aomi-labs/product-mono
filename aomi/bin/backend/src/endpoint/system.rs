use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use aomi_backend::{ChatMessage, MessageSender, SessionManager};
use aomi_core::SystemEvent;

use super::{get_backend_request, types::SystemResponse};

type SharedSessionManager = Arc<SessionManager>;

#[derive(Serialize)]
struct MemoryModeResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

async fn updates_endpoint(
    // Alice: only talke to 1 SSE endpoint -> [title changed, tool complete, etc.]
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    // Allow subscribing even if session doesn't exist yet - will filter by session_id
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx)
        .filter_map(move |result| {
            let update = result.ok()?;
            let matches_session = update
                .get("session_id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == session_id);
            if !matches_session {
                return None;
            }
            Event::default().json_data(&update).ok()
        })
        .map(Ok::<_, Infallible>);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SystemResponse>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    let message = match params.get("message").cloned() {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let requested_backend = get_backend_request(&message);

    let session_state = match session_manager
        .get_or_create_session(&session_id, requested_backend, None)
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

async fn get_async_events_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Value>>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let session_state = session_manager
        .get_session_if_exists(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let events = {
        let mut state = session_state.lock().await;
        state
            .advance_frontend_events()
            .into_iter()
            .filter_map(|event| match event {
                SystemEvent::AsyncCallback(value) => Some(value),
                _ => None,
            })
            .collect()
    };

    Ok(Json(events))
}

async fn memory_mode_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<MemoryModeResponse>, StatusCode> {
    let session_id = match params.get("session_id").cloned() {
        Some(id) => id,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    let memory_mode = params
        .get("memory_mode")
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false);

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
        .route("/events", get(get_async_events_endpoint))
        .route("/system", post(system_message_endpoint))
        .route("/memory-mode", post(memory_mode_endpoint))
}
