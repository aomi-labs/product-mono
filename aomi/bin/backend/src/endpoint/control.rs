use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Extension, Router,
};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tracing::info;

use crate::auth::{AuthorizedKey, SessionId};
use aomi_backend::{SessionManager, DEFAULT_NAMESPACE_SET};

type SharedSessionManager = Arc<SessionManager>;

#[derive(Debug, Serialize)]
pub struct NamespacesResponse {
    pub namespaces: Vec<String>,
    pub source: NamespaceSource,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceSource {
    ApiKey,
    User,
    Default,
}

/// Get the allowed namespaces for the current request context.
///
/// Priority:
/// 1. API key namespaces (if X-API-Key header is present and valid)
/// 2. User namespaces (from query param `public_key` or session cache)
/// 3. Default namespaces
pub async fn get_namespaces_endpoint(
    State(session_manager): State<SharedSessionManager>,
    Extension(SessionId(session_id)): Extension<SessionId>,
    api_key: Option<Extension<AuthorizedKey>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<NamespacesResponse>, StatusCode> {
    info!(session_id, "GET /api/control/namespaces");

    // 1. Check API key namespaces first
    if let Some(Extension(authorized_key)) = api_key {
        let namespaces = authorized_key.get_allowed_namespaces();
        if !namespaces.is_empty() {
            return Ok(Json(NamespacesResponse {
                namespaces,
                source: NamespaceSource::ApiKey,
            }));
        }
    }

    // 2. Get public key: first from query param, then from session cache
    let public_key = params
        .get("public_key")
        .cloned()
        .or_else(|| session_manager.get_public_key(&session_id));

    // 3. Check user namespaces if we have a public key
    if let Some(public_key) = public_key {
        match session_manager.get_user_namespaces(&public_key).await {
            Ok(namespaces) if !namespaces.is_empty() => {
                return Ok(Json(NamespacesResponse {
                    namespaces,
                    source: NamespaceSource::User,
                }));
            }
            Ok(_) => {
                // Empty namespaces, fall through to default
            }
            Err(e) => {
                tracing::warn!(
                    session_id,
                    public_key,
                    error = %e,
                    "Failed to get user namespaces, using default"
                );
            }
        }
    }

    // 4. Return default namespaces
    Ok(Json(NamespacesResponse {
        namespaces: DEFAULT_NAMESPACE_SET.iter().map(|s| s.to_string()).collect(),
        source: NamespaceSource::Default,
    }))
}

pub fn create_control_router() -> Router<SharedSessionManager> {
    Router::new().route("/namespaces", get(get_namespaces_endpoint))
}
