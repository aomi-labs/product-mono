//! Wallet connection API endpoints.

use axum::{
    extract::{Query, Extension},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, Pool};
use std::sync::Arc;
use tracing::{error, info};

use aomi_bot_core::{DbWalletConnectService, WalletConnectService};

/// Shared database pool wrapped in Arc for extension
pub type SharedPool = Arc<Pool<Any>>;

#[derive(Deserialize)]
pub struct SessionQuery {
    session_key: String,
}

#[derive(Serialize)]
pub struct ChallengeResponse {
    challenge: String,
    session_key: String,
}

#[derive(Deserialize)]
pub struct BindRequest {
    session_key: String,
    signature: String,
}

#[derive(Serialize)]
pub struct BindResponse {
    success: bool,
    address: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct WalletStatusResponse {
    connected: bool,
    address: Option<String>,
}

/// GET /api/wallet/challenge?session_key=...
/// Returns a challenge message for the user to sign
pub async fn get_challenge(
    Extension(pool): Extension<SharedPool>,
    Query(params): Query<SessionQuery>,
) -> Result<Json<ChallengeResponse>, (StatusCode, String)> {
    let service = DbWalletConnectService::new((*pool).clone());
    
    match service.generate_challenge(&params.session_key).await {
        Ok(challenge) => {
            info!("Generated challenge for session: {}", params.session_key);
            Ok(Json(ChallengeResponse {
                challenge,
                session_key: params.session_key,
            }))
        }
        Err(e) => {
            error!("Failed to generate challenge: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// POST /api/wallet/bind
/// Verifies signature and binds wallet to session
pub async fn bind_wallet(
    Extension(pool): Extension<SharedPool>,
    Json(req): Json<BindRequest>,
) -> Json<BindResponse> {
    let service = DbWalletConnectService::new((*pool).clone());
    
    match service.verify_and_bind(&req.session_key, &req.signature).await {
        Ok(address) => {
            let address_str = format!("{:?}", address);
            info!("Bound wallet {} to session {}", address_str, req.session_key);
            Json(BindResponse {
                success: true,
                address: Some(address_str),
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to bind wallet: {}", e);
            Json(BindResponse {
                success: false,
                address: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// GET /api/wallet/status?session_key=...
/// Returns the wallet status for a session
pub async fn wallet_status(
    Extension(pool): Extension<SharedPool>,
    Query(params): Query<SessionQuery>,
) -> Json<WalletStatusResponse> {
    let service = DbWalletConnectService::new((*pool).clone());
    
    match service.get_bound_wallet(&params.session_key).await {
        Ok(Some(address)) => Json(WalletStatusResponse {
            connected: true,
            address: Some(address),
        }),
        Ok(None) => Json(WalletStatusResponse {
            connected: false,
            address: None,
        }),
        Err(e) => {
            error!("Failed to get wallet status: {}", e);
            Json(WalletStatusResponse {
                connected: false,
                address: None,
            })
        }
    }
}

/// POST /api/wallet/disconnect
/// Disconnects wallet from session
pub async fn disconnect_wallet(
    Extension(pool): Extension<SharedPool>,
    Json(req): Json<SessionQuery>,
) -> Json<BindResponse> {
    let service = DbWalletConnectService::new((*pool).clone());
    
    match service.disconnect(&req.session_key).await {
        Ok(()) => {
            info!("Disconnected wallet for session {}", req.session_key);
            Json(BindResponse {
                success: true,
                address: None,
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to disconnect wallet: {}", e);
            Json(BindResponse {
                success: false,
                address: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Create wallet router (uses Extension for pool access)
pub fn create_wallet_router() -> Router {
    Router::new()
        .route("/challenge", get(get_challenge))
        .route("/bind", post(bind_wallet))
        .route("/status", get(wallet_status))
        .route("/disconnect", post(disconnect_wallet))
}
