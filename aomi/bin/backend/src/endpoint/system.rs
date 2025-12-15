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

use super::types::SystemResponse;

type SharedSessionManager = Arc<SessionManager>;

#[derive(Serialize)]
struct MemoryModeResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

async fn updates_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx)
        .filter_map(|result| match result {
            Ok(update) => Event::default().json_data(&update).ok(),
            Err(_) => None,
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

    let session_state = match session_manager
        .get_or_create_session(&session_id, None, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    let res = state
        .process_system_message_from_ui(message)
        .await
        .unwrap_or_else(|e| {
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
        state.take_async_events()
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
