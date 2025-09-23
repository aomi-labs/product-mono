use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Sse},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::time::interval;
use tokio_stream::{wrappers::IntervalStream, StreamExt};

use crate::{
    manager::{generate_session_id, SessionManager},
    session::WebStateResponse,
};

type SharedSessionManager = Arc<SessionManager>;

#[derive(Debug, Clone)]
struct NetworkRegistry {
    networks: HashMap<String, NetworkInfo>,
    ordered_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct NetworkInfo {
    name: String,
    port: u16,
    endpoint: String,
}

impl NetworkRegistry {
    fn from_env() -> Self {
        let raw_host = std::env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let accessible_host = if raw_host == "0.0.0.0" {
            "127.0.0.1".to_string()
        } else {
            raw_host.clone()
        };

        let base_port = std::env::var("MCP_SERVER_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(5000);

        let networks_json = std::env::var("MCP_NETWORK_URLS")
            .or_else(|_| std::env::var("MCP_NETWORKS_JSON"))
            .or_else(|_| std::env::var("NETWORK_JSON"))
            .unwrap_or_else(|_| "{\"testnet\":\"http://127.0.0.1:8545\"}".to_string());

        let parsed: HashMap<String, String> = serde_json::from_str(&networks_json).unwrap_or_default();
        let mut names: Vec<String> = parsed.keys().cloned().collect();

        if names.is_empty() {
            names.push("testnet".to_string());
        }

        if !names.iter().any(|name| name == "testnet") {
            names.push("testnet".to_string());
        }

        names.sort();
        if let Some(pos) = names.iter().position(|name| name == "testnet") {
            let entry = names.remove(pos);
            names.insert(0, entry);
        }

        let mut networks = HashMap::new();
        let mut ordered_names = Vec::new();

        for (index, name) in names.into_iter().enumerate() {
            if let Some(port) = base_port.checked_add(index as u16) {
                let endpoint = format!("http://{}:{}", accessible_host, port);
                networks.insert(
                    name.to_lowercase(),
                    NetworkInfo {
                        name: name.clone(),
                        port,
                        endpoint,
                    },
                );
                ordered_names.push(name);
            }
        }

        Self {
            networks,
            ordered_names,
        }
    }

    fn get(&self, network: &str) -> Option<&NetworkInfo> {
        self.networks.get(&network.to_lowercase())
    }

    fn available(&self) -> Vec<String> {
        self.ordered_names.clone()
    }
}

static NETWORK_REGISTRY: LazyLock<NetworkRegistry> = LazyLock::new(NetworkRegistry::from_env);

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
struct McpNetworkRequest {
    network: String,
    session_id: Option<String>,
}

#[derive(Deserialize)]
struct InterruptRequest {
    session_id: Option<String>,
}

#[derive(Serialize)]
struct McpNetworkResponse {
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

    if let Err(_) = state.process_message_from_ui(request.message).await {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(state.get_state()))
}

async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<WebStateResponse>, StatusCode> {
    let session_id = params.get("session_id").cloned().unwrap_or_else(generate_session_id);

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
    let session_id = params.get("session_id").cloned().unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => {
            let dummy_state =
                Arc::new(tokio::sync::Mutex::new(
                    crate::session::SessionState::new(session_manager.skip_docs())
                        .await
                        .unwrap_or_else(|_| {
                            panic!("Failed to create even a fallback session")
                        }),
                ));
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
    if let Err(_) = state.interrupt_processing().await {
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

async fn mcp_network_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(request): Json<McpNetworkRequest>,
) -> Result<Json<McpNetworkResponse>, StatusCode> {
    let session_id = request.session_id.unwrap_or_else(generate_session_id);

    let session_state = match session_manager.get_or_create_session(&session_id).await {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    let requested_network = request.network.trim();

    if requested_network.is_empty() {
        let available_networks = NETWORK_REGISTRY.available();
        return Ok(Json(McpNetworkResponse {
            success: false,
            message: "Network name is required".to_string(),
            data: Some(serde_json::json!({
                "available": available_networks,
            })),
        }));
    }

    let registry = &*NETWORK_REGISTRY;
    match registry.get(requested_network) {
        Some(info) => {
            state.add_system_message(&format!(
                "🔄 MCP network set to {} (port {})",
                info.name, info.port
            ));

            let available_networks = registry.available();
            Ok(Json(McpNetworkResponse {
                success: true,
                message: format!("MCP network set to {}", info.name),
                data: Some(serde_json::json!({
                    "network": info.name,
                    "endpoint": info.endpoint,
                    "port": info.port,
                    "available": available_networks,
                })),
            }))
        }
        None => {
            let available_networks = registry.available();
            Ok(Json(McpNetworkResponse {
                success: false,
                message: format!("Unknown network: {}", requested_network),
                data: Some(serde_json::json!({
                "available": available_networks,
            })),
            }))
        }
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
        .route("/api/mcp/network", post(mcp_network_endpoint))
        .with_state(session_manager)
}
