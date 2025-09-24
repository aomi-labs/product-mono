use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Sse},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};

use crate::{
    manager::{generate_session_id, SharedSessionManager},
    session::{SessionResponse, SessionState},
};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SystemMessageRequest {
    pub message: String,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct McpCommandRequest {
    pub command: String,
    pub args: serde_json::Value,
    pub session_id: Option<String>,
}

#[derive(Deserialize)]
pub struct InterruptRequest {
    pub session_id: Option<String>,
}

#[derive(Serialize)]
pub struct McpCommandResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

pub async fn health() -> &'static str {
    "OK"
}

pub async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    match session_manager.process_message_with_pool(&session_id, request.message).await {
        Ok(state) => Ok(Json(state)),
        Err(err) => {
            eprintln!("❌ Failed to process chat message: {err}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(generate_session_id);

    let session_state =
        session_manager.get_or_create_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut state = session_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

pub async fn chat_stream(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => Arc::new(Mutex::new(SessionState::new(false).await.unwrap())),
    };

    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let session_state = Arc::clone(&session_state);
            async move {
                let mut state = session_state.lock().await;
                state.update_state().await;
                let response = state.get_state();
                axum::response::sse::Event::default().json_data(&response).map_err(|_| ())
            }
        })
        .then(|f| f)
        .map(|result| result.map_err(|_| unreachable!()));

    Sse::new(stream)
}

pub async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<InterruptRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state =
        session_manager.get_or_create_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut state = session_state.lock().await;
    state.interrupt_processing().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(state.get_state()))
}

pub async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<SystemMessageRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state =
        session_manager.get_or_create_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut state = session_state.lock().await;
    state.append_system(&request.message);
    Ok(Json(state.get_state()))
}

pub async fn mcp_command_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<McpCommandRequest>,
) -> Result<Json<McpCommandResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state =
        session_manager.get_or_create_session(&session_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut state = session_state.lock().await;

    match request.command.as_str() {
        "set_network" => {
            let network_name = request.args.get("network").and_then(|v| v.as_str()).unwrap_or("testnet");
            state.append_system(&format!("🔄 Attempting to switch network to {}", network_name));
            Ok(Json(McpCommandResponse {
                success: true,
                message: format!("Network switch to {} initiated", network_name),
                data: Some(serde_json::json!({ "network": network_name })),
            }))
        }
        _ => Ok(Json(McpCommandResponse {
            success: false,
            message: format!("Unknown command: {}", request.command),
            data: None,
        })),
    }
}
