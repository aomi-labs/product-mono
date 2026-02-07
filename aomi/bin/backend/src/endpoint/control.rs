use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::SessionId;
use aomi_backend::{AomiModel, AuthorizedKey, Namespace, NamespaceAuth, Selection, SessionManager};

pub type SharedSessionManager = Arc<SessionManager>;

/// HTTP endpoint to get allowed namespaces for the current request context.
pub async fn get_namespaces_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    api_key: Option<Extension<AuthorizedKey>>,
    params: Option<Query<HashMap<String, String>>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    info!(session_id, "GET /api/control/namespaces");

    let public_key = params
        .as_ref()
        .and_then(|p| p.get("public_key").cloned())
        .or_else(|| session_manager.get_public_key(&session_id));

    let mut auth = NamespaceAuth::new(public_key, api_key.map(|e| e.0), None);
    auth.resolve(&session_manager).await;

    Ok(Json(auth.current_authorization))
}

/// HTTP endpoint to get available models.
pub async fn get_model_endpoint(
    Extension(SessionId(_session_id)): Extension<SessionId>,
) -> Json<Vec<&'static str>> {
    Json(AomiModel::rig_all().iter().map(|m| m.rig_label()).collect())
}

/// HTTP endpoint to set the model selection for a session.
pub async fn set_model_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let rig_str = params.get("rig").ok_or(StatusCode::BAD_REQUEST)?;
    // baml is optional, defaults to same as rig
    let baml_str = params.get("baml").unwrap_or(rig_str);

    info!(session_id, rig = %rig_str, baml = %baml_str, "POST /api/control/model");

    let rig = AomiModel::parse_rig(rig_str).ok_or_else(|| {
        tracing::warn!(session_id, rig = %rig_str, "Invalid rig model");
        StatusCode::BAD_REQUEST
    })?;

    let baml = AomiModel::parse_baml(baml_str).ok_or_else(|| {
        tracing::warn!(session_id, baml = %baml_str, "Invalid baml model");
        StatusCode::BAD_REQUEST
    })?;

    let selection = Selection { rig, baml };

    let namespace = params
        .get("namespace")
        .and_then(|s| Namespace::parse(s))
        .unwrap_or(Namespace::Default);

    let public_key = params
        .get("public_key")
        .cloned()
        .or_else(|| session_manager.get_public_key(&session_id));

    // get_or_create_session handles auth check and ensure_backend internally
    let mut auth = NamespaceAuth::new(public_key, None, Some(namespace.as_str()));

    session_manager
        .get_or_create_session(&session_id, &mut auth, Some(selection))
        .await
        .map_err(|e| {
            tracing::warn!(session_id, error = %e, "Failed to set model selection");
            StatusCode::FORBIDDEN
        })?;

    Ok(Json(json!({
        "success": true,
        "rig": rig.rig_slug(),
        "baml": baml.baml_client_name()
    })))
}

pub fn create_control_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/namespaces", get(get_namespaces_endpoint))
        .route("/models", get(get_model_endpoint))
        .route("/model", post(set_model_endpoint))
}
