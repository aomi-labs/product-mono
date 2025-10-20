use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Sse},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt};

use crate::{
    manager::{generate_session_id, SessionManager},
    session::WebStateResponse,
};

type SharedSessionManager = Arc<SessionManager>;

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct SystemMessageRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct InterruptRequest {
    session_id: Option<String>,
}

#[derive(Serialize)]
struct McpCommandResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

async fn health() -> &'static str {
    "OK"
}

async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    if state
        .process_message_from_ui(request.message)
        .await
        .is_err()
    {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = params
        .get("session_id")
        .cloned()
        .unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

async fn chat_stream(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    let session_id = params
        .get("session_id")
        .cloned()
        .unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => {
            let chat_app = session_manager.chat_app();
            let history = Arc::new(tokio::sync::Mutex::new(Vec::new()));
            Arc::new(tokio::sync::Mutex::new(
                crate::session::SessionState::new(chat_app, history)
                    .await
                    .unwrap_or_else(|_| panic!("Failed to create even a fallback session")),
            ))
        }
    };

    let stream = IntervalStream::new(interval(Duration::from_millis(100)))
        .map(move |_| {
            let session_state = Arc::clone(&session_state);
            async move {
                let mut state = session_state.lock().await;
                state.update_state().await;
                let response = state.get_state();

                axum::response::sse::Event::default()
                    .json_data(&response)
                    .map_err(|_| ())
            }
        })
        .then(|f| f)
        .map(|result| result.map_err(|_| unreachable!()));

    Sse::new(stream)
}

async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<InterruptRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if state.interrupt_processing().await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<SystemMessageRequest>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    state.add_system_message(&request.message);

    let system_message_for_agent = format!("[[SYSTEM:{}]]", request.message);
    let _ = state.send_to_llm().try_send(system_message_for_agent);

    Ok(Json(state.get_state()))
}

async fn mcp_command_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<McpCommandRequest>,
) -> Result<Json<McpCommandResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    match request.command.as_str() {
        "set_network" => {
            let network_name = request
                .args
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("testnet");
            let command_message = format!("set_network {}", network_name);

            if let Err(e) = state.send_to_llm().send(command_message).await {
                return Ok(Json(McpCommandResponse {
                    success: false,
                    message: format!("Failed to send command to agent: {}", e),
                    data: None,
                }));
            }

            state.add_system_message(&format!(
                "🔄 Attempting to switch network to {}",
                network_name
            ));

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

pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/mcp-command", post(mcp_command_endpoint))
        .with_state(session_manager)
}
