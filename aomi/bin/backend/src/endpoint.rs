use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::time::interval;
use tokio_stream::{wrappers::{BroadcastStream, IntervalStream}, StreamExt};

use aomi_backend::{
    generate_session_id,
    session::{HistorySession, SystemResponse},
    BackendType, ChatMessage, MessageSender, SessionManager, SessionResponse,
};

type SharedSessionManager = Arc<SessionManager>;

#[derive(Serialize)]
struct MemoryModeResponse {
    success: bool,
    message: String,
    data: Option<serde_json::Value>,
}

async fn health() -> &'static str {
    "OK"
}

fn get_backend_request(message: &str) -> Option<BackendType> {
    let normalized = message.to_lowercase();
    if normalized.contains("l2b-magic-off") {
        Some(BackendType::Default)
    } else if normalized.contains("l2beat-magic") {
        Some(BackendType::L2b)
    } else {
        None
    }
}

async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SessionResponse>, StatusCode> {
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

    let session_state = match session_manager
        .get_or_create_session(&session_id, get_backend_request(&message), None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    if state.process_user_message(message).await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(Json(state.get_state()))
}

async fn state_endpoint(
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

    let public_key = params.get("public_key").cloned();
    session_manager
        .set_session_public_key(&session_id, public_key.clone())
        .await;

    let session_state = session_manager
        .get_or_create_session(&session_id, None, None)
        .await
        .unwrap();

    // 200 -> [...........] [..... .......] -> {... .... ...... ... } // managed by FE npm lib
    // 100 -> [.....] [.....] [.....] [...]-> { ... ... ... ... } // managed by FE npm lib

    let stream = IntervalStream::new(interval(Duration::from_millis(100))).then(move |_| {
        let session_state = Arc::clone(&session_state);

        let session_id = session_id.clone();
        let session_manager = session_manager.clone();
        let public_key = public_key.clone();

        async move {
            let response = {
                let mut state = session_state.lock().await;
                state.update_state().await;
                state.get_state()
            };

            session_manager
                .update_user_history(&session_id, public_key.clone(), &response.messages)
                .await;

            Event::default()
                .json_data(&response)
                .map_err(|_| unreachable!())
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
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

    Ok(Json(state.get_state()))
}

async fn updates_endpoint(
    State(session_manager): State<SharedSessionManager>,
) -> Result<Sse<impl StreamExt<Item = Result<Event, Infallible>>>, StatusCode> {
    let rx = session_manager.subscribe_to_updates();

    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(update) => Event::default()
                .json_data(&update)
                .ok(),
            Err(_) => None,
        }
    }).map(|event| Ok::<_, Infallible>(event));

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
        .process_system_message(message)
        .await
        .unwrap_or_else(|e| {
            ChatMessage::new(MessageSender::System, e.to_string(), Some("System Error"))
        });

    Ok(Json(SystemResponse { res }))
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

async fn session_list_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<HistorySession>>, StatusCode> {
    let public_key = match params.get("public_key").cloned() {
        Some(pk) => pk,
        None => return Err(StatusCode::BAD_REQUEST),
    };
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    session_manager
        .get_history_sessions(&public_key, limit)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn session_create_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session_id = generate_session_id();
    let public_key = payload.get("public_key").cloned();

    // Get title from frontend, or use truncated session_id as fallback
    let title = payload.get("title").cloned().or_else(|| {
        let mut placeholder = session_id.clone();
        placeholder.truncate(6);
        Some(placeholder)
    });

    session_manager
        .set_session_public_key(&session_id, public_key.clone())
        .await;

    let session_state = session_manager
        .get_or_create_session(&session_id, None, title.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get actual title from session state (might be None if creation failed)
    let final_title = {
        let state = session_state.lock().await;
        state.get_title().map(|s| s.to_string())
    };

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "title": final_title.or(title),
    })))
}

async fn session_archive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager
        .set_session_archived(&session_id, true)
        .await;
    Ok(StatusCode::OK)
}

async fn session_unarchive_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager
        .set_session_archived(&session_id, false)
        .await;
    Ok(StatusCode::OK)
}

async fn session_delete_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    session_manager
        .delete_session(&session_id)
        .await;
    Ok(StatusCode::OK)
}

async fn session_rename_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Path(session_id): Path<String>,
    Json(payload): Json<HashMap<String, String>>,
) -> Result<StatusCode, StatusCode> {
    let title = match payload.get("title").cloned() {
        Some(t) => t,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    session_manager
        .update_session_title(&session_id, title)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

pub fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/chat", post(chat_endpoint))
        .route("/api/state", get(state_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        .route("/api/interrupt", post(interrupt_endpoint))
        .route("/api/updates", get(updates_endpoint))
        .route("/api/system", post(system_message_endpoint))
        .route("/api/memory-mode", post(memory_mode_endpoint))
        .route(
            "/api/sessions",
            get(session_list_endpoint).post(session_create_endpoint),
        )
        .route(
            "/api/sessions/:session_id",
            delete(session_delete_endpoint).patch(session_rename_endpoint),
        )
        .route(
            "/api/sessions/:session_id/archive",
            post(session_archive_endpoint),
        )
        .route(
            "/api/sessions/:session_id/unarchive",
            post(session_unarchive_endpoint),
        )
        .with_state(session_manager)
}
