use axum::{
    Extension, extract::{Query, State}, http::StatusCode, response::Json,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::{AuthorizedKey, SessionId};
use crate::endpoint::history;
use aomi_backend::{
    Namespace, SessionManager, SessionResponse,
    extract_namespace, get_backend_request, is_not_default,
};

pub type SharedSessionManager = Arc<SessionManager>;

/// Returns the first N words of a string for logging preview
fn first_n_words(s: &str, n: usize) -> String {
    s.split_whitespace()
        .take(n)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Check namespace authorization. Returns Err if unauthorized.
fn check_namespace_auth(
    namespace: &str,
    api_key: Option<Extension<AuthorizedKey>>,
) -> Result<(), StatusCode> {
    if is_not_default(namespace) {
        let Extension(key) = api_key.ok_or(StatusCode::UNAUTHORIZED)?;
        if !key.allows_namespace(namespace) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

pub async fn health() -> &'static str {
    "OK"
}

pub async fn chat_endpoint(
    State(session_manager): State<SharedSessionManager>,
    api_key: Option<Extension<AuthorizedKey>>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<SessionResponse>, StatusCode> {
    let namespace = extract_namespace(params.get("namespace"));
    check_namespace_auth(namespace, api_key)?;

    let public_key = params.get("public_key").cloned();
    let message = match params.get("message").cloned() {
        Some(m) => m,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let preview = first_n_words(&message, 3);
    info!(session_id, namespace, preview, "POST /api/chat");

    session_manager
        .set_session_public_key(&session_id, public_key.clone())
        .await;

    let backend_request = Namespace::from_str(namespace)
        .or_else(|| get_backend_request(&message));

    let session_state = match session_manager
        .get_or_create_session(&session_id, backend_request)
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
    let response = state.format_session_response(title);
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
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!(session_id, "GET /api/state");

    let session_state = match session_manager
        .get_or_create_session(&session_id, None)
        .await
    {
        Ok(state) => state,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let mut state = session_state.lock().await;
    state.sync_state().await;
    let title = session_manager.get_session_title(&session_id);
    let response = state.format_session_response(title);
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
    }

    Ok(Json(body))
}

pub async fn interrupt_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
) -> Result<Json<SessionResponse>, StatusCode> {
    info!(session_id, "POST /api/interrupt");

    let session_state = match session_manager
        .get_or_create_session(&session_id, None)
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
    let response = state.format_session_response(title);
    drop(state);

    Ok(Json(response))
}
