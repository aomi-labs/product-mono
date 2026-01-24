mod db;
mod history;
mod sessions;
mod system;
mod types;
mod chat;

use axum::{
    routing::{get, post},
    Extension, Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use chat::{chat_endpoint, state_endpoint, interrupt_endpoint};
use crate::auth::{requires_namespace_auth, AuthorizedKey, SessionId, DEFAULT_NAMESPACE};
use aomi_backend::{Namespace, SessionManager, SessionResponse};


async fn health() -> &'static str {
    "OK"
}

async fn chat_endpoint_(
    State(session_manager): State<SharedSessionManager>,
    api_key: Option<Extension<AuthorizedKey>>,
    Extension(SessionId(session_id)): Extension<SessionId>,
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

async fn state_endpoint_(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
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

async fn interrupt_endpoint_(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<SessionResponse>, StatusCode> {
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
