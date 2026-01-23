
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

use aomi_backend::{Namespace, SessionManager, SessionResponse, generate_session_id};

use crate::endpoint::history;

type SharedSessionManager = Arc<SessionManager>;

pub async fn health() -> &'static str {
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

#[allow(dead_code)]
pub async fn chat_endpoint(
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

pub async fn state_endpoint(
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

    // Parse and sync user state from frontend if provided
    if let Some(user_state_json) = params.get("user_state") {
        if let Ok(user_state) = serde_json::from_str::<aomi_backend::UserState>(user_state_json) {
            state.sync_user_state(user_state).await;
        }
    }

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

pub async fn interrupt_endpoint(
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
