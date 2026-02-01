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
use aomi_backend::{
    AomiModel, AuthorizedKey, NamespaceAuth, Namespace, Selection, SessionManager,
};

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

    let mut auth = NamespaceAuth::new(public_key.clone(), api_key.map(|e| e.0), None);

    let user_namespaces = if let Some(ref pk) = public_key {
        session_manager.get_user_namespaces(pk).await.ok()
    } else {
        None
    };
    auth.merge_authorization(user_namespaces);

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
    let baml_str = params.get("baml").ok_or(StatusCode::BAD_REQUEST)?;

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

    let created = session_manager
        .ensure_backend(namespace, selection)
        .await
        .map_err(|e| {
            tracing::error!(session_id, error = %e, "Failed to ensure backend");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut auth = NamespaceAuth::new(None, None, Some(namespace.as_str()));
    if let Err(e) = session_manager
        .get_or_create_session(&session_id, &mut auth)
        .await
    {
        tracing::error!(session_id, error = %e, "Failed to initialize session");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(json!({
        "success": true,
        "rig": rig.rig_slug(),
        "baml": baml.baml_client_name(),
        "created": created
    })))
}

pub fn create_control_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/namespaces", get(get_namespaces_endpoint))
        .route("/models", get(get_model_endpoint))
        .route("/model", post(set_model_endpoint))
}
