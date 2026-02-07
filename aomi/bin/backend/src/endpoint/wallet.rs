use std::sync::Arc;

use aomi_backend::SessionManager;
use axum::{
    http::{HeaderMap, StatusCode},
    routing::post,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{AnyPool, Row};
use tracing::{info, warn};

pub type SharedSessionManager = Arc<SessionManager>;

#[derive(Debug, Deserialize)]
pub struct WalletBindRequest {
    wallet_address: Option<String>,
    platform: Option<String>,
    platform_user_id: Option<String>,
    init_data: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WalletBindResponse {
    success: bool,
    wallet_address: String,
    session_key: String,
}

fn err(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message })))
}

fn is_valid_evm_address(address: &str) -> bool {
    address.len() == 42
        && address.starts_with("0x")
        && address.as_bytes()[2..]
            .iter()
            .all(|b| b.is_ascii_hexdigit())
}

pub async fn bind_wallet_endpoint(
    Extension(pool): Extension<AnyPool>,
    headers: HeaderMap,
    Json(payload): Json<WalletBindRequest>,
) -> Result<Json<WalletBindResponse>, (StatusCode, Json<Value>)> {
    let expected_internal_key = std::env::var("WALLET_BIND_INTERNAL_KEY")
        .or_else(|_| std::env::var("TELEGRAM_BOT_TOKEN"))
        .map_err(|_| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Wallet bind internal key is not configured",
            )
        })?;

    let provided_internal_key = headers
        .get("x-wallet-bind-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided_internal_key != expected_internal_key {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "Unauthorized wallet bind request",
        ));
    }

    let wallet_address = payload
        .wallet_address
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "Missing wallet_address"))?
        .to_string();
    let platform = payload
        .platform
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "Missing platform"))?
        .to_string();
    let platform_user_id = payload
        .platform_user_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "Missing platform_user_id"))?
        .to_string();

    if !is_valid_evm_address(&wallet_address) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Invalid wallet address format",
        ));
    }

    if platform.eq_ignore_ascii_case("telegram")
        && payload
            .init_data
            .as_deref()
            .map(str::trim)
            .is_some_and(|v| !v.is_empty())
    {
        info!("Received Telegram init_data for wallet bind");
    }

    let session_key = format!("{}:dm:{}", platform, platform_user_id);

    let existing = sqlx::query("SELECT public_key FROM sessions WHERE id = $1 LIMIT 1")
        .bind(&session_key)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("DB error: {}", e),
            )
        })?;
    let previous_wallet = existing
        .and_then(|row| row.try_get::<Option<String>, _>("public_key").ok())
        .flatten();
    let wallet_changed = match previous_wallet {
        Some(prev) => !prev.eq_ignore_ascii_case(&wallet_address),
        None => true,
    };

    sqlx::query(
        r#"
        INSERT INTO users (public_key, username, created_at)
        VALUES ($1, NULL, EXTRACT(EPOCH FROM NOW())::BIGINT)
        ON CONFLICT (public_key) DO NOTHING
        "#,
    )
    .bind(&wallet_address)
    .execute(&pool)
    .await
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("DB error: {}", e),
        )
    })?;

    sqlx::query(
        r#"
        INSERT INTO sessions (id, public_key, started_at, last_active_at, title, pending_transaction)
        VALUES ($1, $2, EXTRACT(EPOCH FROM NOW())::BIGINT, EXTRACT(EPOCH FROM NOW())::BIGINT, NULL, NULL)
        ON CONFLICT (id)
        DO UPDATE SET public_key = $2
        "#,
    )
    .bind(&session_key)
    .bind(&wallet_address)
    .execute(&pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("DB error: {}", e)))?;

    if platform.eq_ignore_ascii_case("telegram") && wallet_changed {
        if let Ok(bot_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
            if !bot_token.trim().is_empty() {
                let client = reqwest::Client::new();
                let response = client
                    .post(format!(
                        "https://api.telegram.org/bot{}/sendMessage",
                        bot_token
                    ))
                    .json(&json!({
                        "chat_id": platform_user_id,
                        "text": format!("✅ Wallet connected: {}", wallet_address),
                    }))
                    .send()
                    .await;

                match response {
                    Ok(resp) if !resp.status().is_success() => {
                        warn!(
                            status = %resp.status(),
                            "Failed to send Telegram wallet confirmation"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "Failed to send Telegram wallet confirmation");
                    }
                }
            } else {
                warn!("TELEGRAM_BOT_TOKEN is empty; skipping Telegram confirmation");
            }
        }
    }

    Ok(Json(WalletBindResponse {
        success: true,
        wallet_address,
        session_key,
    }))
}

pub fn create_wallet_router() -> Router<SharedSessionManager> {
    Router::new().route("/bind", post(bind_wallet_endpoint))
}
