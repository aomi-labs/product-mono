use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::SessionId;
use crate::endpoint::history;
use aomi_backend::{AuthorizedKey, NamespaceAuth, SessionManager, SessionResponse, UserState};

pub type SharedSessionManager = Arc<SessionManager>;

/// Returns the first N words of a string for logging preview
fn first_n_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

pub async fn health() -> &'static str {
    "OK"
}

pub async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    api_key: Option<Extension<AuthorizedKey>>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let requested_namespace = params.get("namespace").map(String::as_str);
    let public_key = params.get("public_key").cloned();
    let message = match params.get("message").cloned() {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let preview = first_n_words(&message, 3);
    info!(
        session_id,
        namespace = requested_namespace.unwrap_or("default"),
        preview,
        "POST /api/chat"
    );

    // Form NamespaceAuth with default authorization
    let mut auth = NamespaceAuth::new(public_key, api_key.map(|e| e.0), requested_namespace);

    // Get or create session (merges authorization and validates namespace)
    // Pass None for selection to preserve the session's current model selection
    let session_state = match session_manager
        .get_or_create_session(&session_id, &mut auth, None)
        .await
    {
        Ok(state) => state,
        Err(e) => {
            tracing::warn!(session_id, error = %e, "Failed to get or create session");
            return Err(StatusCode::FORBIDDEN);
        }
    };

    let mut state = session_state.lock().await;

    // Sync user state (wallet connection, etc.) before processing message
    if let Some(user_state) = params.get("user_state") {
        if let Ok(parsed_state) = serde_json::from_str::<UserState>(user_state) {
            tracing::info!(session_id, "Syncing user state: {:?}", parsed_state);
            state.sync_user_state(parsed_state).await;
        }
    }

    if state.send_user_input(message).await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let title = session_manager.get_session_title(&session_id);
    let response = state.format_session_response(title);
    let user_state_snapshot = state.user_state.read().await.clone();
    drop(state);

    history::maybe_update_history(
        &session_manager,
        &session_id,
        &response.messages,
        response.is_processing,
    )
    .await;

    let mut value = serde_json::to_value(response).unwrap_or_else(|_| json!({}));
    value["user_state"] = serde_json::to_value(user_state_snapshot).unwrap_or(json!(null));

    Ok(Json(value))
}

pub async fn state_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!(session_id, "GET /api/state");

    // State endpoint only reads existing session, doesn't create or switch backends
    // Return empty state if session doesn't exist (common on initial page load)
    let session_state = match session_manager.get_session_if_exists(&session_id) {
        Some(state) => state,
        None => {
            return Ok(Json(json!({
                "messages": [],
                "system_events": [],
                "title": null,
                "is_processing": false,
                "user_state": null
            })));
        }
    };

    let mut state = session_state.lock().await;
    if let Some(user_state) = params.get("user_state") {
        let parsed_state: UserState =
            serde_json::from_str(user_state).map_err(|_| StatusCode::BAD_REQUEST)?;
        state.sync_user_state(parsed_state).await;
    }
    state.sync_state().await;
    let title = session_manager.get_session_title(&session_id);
    let response = state.format_session_response(title);
    let user_state_snapshot = state.user_state.read().await.clone();
    drop(state);

    history::maybe_update_history(
        &session_manager,
        &session_id,
        &response.messages,
        response.is_processing,
    )
    .await;

    let mut value = serde_json::to_value(response).unwrap_or_else(|_| json!({}));
    value["user_state"] = serde_json::to_value(user_state_snapshot).unwrap_or(json!(null));
    Ok(Json(value))
}

pub async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<SessionResponse>, StatusCode> {
    info!(session_id, "POST /api/interrupt");

    // Interrupt endpoint only reads existing session, doesn't create or switch backends
    let session_state = match session_manager.get_session_if_exists(&session_id) {
        Some(state) => state,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let mut state = session_state.lock().await;
    if state.interrupt_processing().await.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let title = session_manager.get_session_title(&session_id);
    let response = state.format_session_response(title);
    drop(state);

    Ok(Json(response))
}
