use anyhow::Result;
// Environment variables
static BACKEND_HOST: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()));
static BACKEND_PORT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::env::var("BACKEND_PORT").unwrap_or_else(|_| "8080".to_string()));

use axum::{
    extract::State,
    http::StatusCode,
    response::{Json, Sse},
};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::interval};
use tokio_stream::{wrappers::IntervalStream, StreamExt};

use crate::manager::*;
use crate::session::*;

// API Types
#[derive(Deserialize)]
pub(crate) struct ChatRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SystemMessageRequest {
    message: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct McpCommandRequest {
    command: String,
    args: serde_json::Value,
    session_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct InterruptRequest {
    session_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct McpCommandResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

type SharedSessionManager = Arc<SessionManager>;

// HTTP Handlers
pub(crate) async fn health() -> &'static str {
    "OK"
}

pub(crate) async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<ChatRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    if let Err(_) = state.process_msg_from_ui(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

pub(crate) async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Json<SessionResponse>, StatusCode> {
    // For backward compatibility, create a default session if no session_id provided
    let session_id = generate_session_id();

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    state.update_state().await;
    Ok(Json(state.get_state()))
}

pub(crate) async fn chat_stream(
    State(session_manager): State<SharedSessionManager>,
) -> Sse<impl StreamExt<Item = Result<axum::response::sse::Event, Infallible>>> {
    // For backward compatibility, create a default session if no session_id provided
    let session_id = generate_session_id();

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => {
            // Return simple error stream - just create a dummy session for error case
            let dummy_state = Arc::new(Mutex::new(SessionState::new(false).await.unwrap_or_else(|_| {
                // This is a fallback - should not happen in practice
                panic!("Failed to create even a fallback session")
            })));
            dummy_state
        }
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

pub(crate) async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<InterruptRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if let Err(_) = state.interrupt_processing().await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

pub(crate) async fn system_message_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<SystemMessageRequest>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    // Add system message to chat display
    state.append_system(&request.message);

    // Format message with [[SYSTEM:]] marker and send to agent for processing
    let system_message_for_agent = format!("[[SYSTEM:{}]]", request.message);

    // Send to agent (non-blocking, ignore errors as agent might be busy)
    let _ = state.sender_to_llm.try_send(system_message_for_agent);

    Ok(Json(state.get_state()))
}

pub(crate) async fn mcp_command_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<McpCommandRequest>,
) -> Result<Json<McpCommandResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id, false).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;

    // Handle different MCP commands
    match request.command.as_str() {
        "set_network" => {
            // Extract network name from args
            let network_name = request.args.get("network").and_then(|v| v.as_str()).unwrap_or("testnet");

            // Create the set_network command message
            let command_message = format!("set_network {}", network_name);

            // Send the command through the agent
            if let Err(e) = state.sender_to_llm.send(command_message).await {
                return Ok(Json(McpCommandResponse {
                    success: false,
                    message: format!("Failed to send command to agent: {}", e),
                    data: None,
                }));
            }

            // Add system message to indicate network switch attempt
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
