use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

use crate::auth::{ApiAuth, AuthorizedKey, API_KEY_HEADER};
use crate::endpoint::chat::SharedSessionManager;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct NamespacesQuery {
    pub public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetModelRequest {
    pub rig: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetModelResponse {
    pub success: bool,
    pub rig: String,
    pub baml: String,
    pub created: bool,
}

// ============================================================================
// Base namespaces all users have access to
// ============================================================================

const BASE_NAMESPACES: &[&str] = &["default", "polymarket"];

// ============================================================================
// Available models
// ============================================================================

const AVAILABLE_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "claude-opus-4-20250514",
    "gpt-4.1-2025-04-14",
    "gpt-4.1-mini-2025-04-14",
    "gpt-4.1-nano-2025-04-14",
    "o3-mini-2025-01-31",
];

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/control/namespaces
/// Returns merged namespaces: base namespaces + API key namespaces (if provided)
pub async fn namespaces_endpoint(
    api_key: Option<Extension<AuthorizedKey>>,
    Query(_query): Query<NamespacesQuery>,
) -> Json<Vec<String>> {
    let mut namespaces: HashSet<String> = BASE_NAMESPACES
        .iter()
        .map(|s| s.to_string())
        .collect();

    // If API key is provided (via middleware), merge with its allowed namespaces
    if let Some(Extension(authorized)) = api_key {
        for ns in authorized.allowed_namespaces() {
            namespaces.insert(ns);
        }
    }

    // TODO: If public_key is provided, could fetch user-specific namespaces from DB

    let mut result: Vec<String> = namespaces.into_iter().collect();
    result.sort();
    Json(result)
}

/// GET /api/control/models
/// Returns list of available models
pub async fn models_endpoint() -> Json<Vec<String>> {
    Json(AVAILABLE_MODELS.iter().map(|s| s.to_string()).collect())
}

/// POST /api/control/model
/// Sets the model for a session
pub async fn set_model_endpoint(
    Json(request): Json<SetModelRequest>,
) -> Json<SetModelResponse> {
    // For now, just acknowledge the model selection
    // TODO: Store in session state or database
    Json(SetModelResponse {
        success: true,
        rig: request.rig.clone(),
        baml: format!("client<llm> {}", request.rig),
        created: false,
    })
}

// ============================================================================
// Router
// ============================================================================

pub fn create_control_router() -> Router<SharedSessionManager> {
    Router::new()
        .route("/namespaces", get(namespaces_endpoint))
        .route("/models", get(models_endpoint))
        .route("/model", post(set_model_endpoint))
}
