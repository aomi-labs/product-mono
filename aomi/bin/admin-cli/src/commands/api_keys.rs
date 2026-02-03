use anyhow::{Context, Result, bail};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use sha2::Sha256;

use crate::cli::{ApiKeyCreateArgs, ApiKeyListArgs, ApiKeyUpdateArgs};
use crate::util::print_json;
use aomi_tools::db::{ApiKeyStore, ApiKeyStoreApi, ApiKeyUpdate};

/// Static signing key for API key generation (HMAC-SHA256)
static NAMESPACE_SIGNER: &[u8] = b"aomi-api-key-signer-v1-2026-foameo";

pub async fn create_api_key(args: ApiKeyCreateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let namespaces = normalize_namespaces(args.namespaces)?;
    let api_key = args.key.unwrap_or_else(|| generate_api_key(&namespaces));
    let store = ApiKeyStore::new(pool.clone());

    let row = store
        .create_api_key(api_key, args.label, namespaces)
        .await
        .context("failed to insert api key")?;

    print_json(&api_key_to_json(&row)?)?;
    Ok(())
}

pub async fn list_api_keys(args: ApiKeyListArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = ApiKeyStore::new(pool.clone());
    let rows = store
        .list_api_keys(args.active_only, args.limit, args.offset)
        .await?;
    let json_rows = rows
        .iter()
        .map(api_key_to_json)
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_api_key(args: ApiKeyUpdateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    if args.clear_label && args.label.is_some() {
        bail!("cannot set --label and --clear-label together");
    }

    if args.active && args.inactive {
        bail!("cannot set --active and --inactive together");
    }

    let update = ApiKeyUpdate {
        api_key: args.api_key,
        label: args.label,
        clear_label: args.clear_label,
        allowed_namespaces: match args.namespaces {
            Some(namespaces) => Some(normalize_namespaces(Some(namespaces))?),
            None => None,
        },
        is_active: if args.active || args.inactive {
            Some(args.active)
        } else {
            None
        },
    };
    let store = ApiKeyStore::new(pool.clone());
    let row = store.update_api_key(update).await?;
    print_json(&api_key_to_json(&row)?)?;
    Ok(())
}

fn normalize_namespaces(namespaces: Option<Vec<String>>) -> Result<Vec<String>> {
    let ns = namespaces.context("namespaces required")?;
    let values: Vec<String> = ns
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if values.is_empty() {
        bail!("no namespaces provided");
    }

    Ok(values)
}

/// Generate API key: aomi-{sign(random32 + namespaces)}
/// Format: aomi-{first 32 chars of HMAC-SHA256 hex}
fn generate_api_key(namespaces: &[String]) -> String {
    // Generate 32 random bytes
    let mut random_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut random_bytes);

    // Build message: random bytes + all namespaces concatenated
    let mut message = random_bytes.to_vec();
    for ns in namespaces {
        message.extend_from_slice(ns.as_bytes());
    }

    // Sign with HMAC-SHA256
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(NAMESPACE_SIGNER)
        .expect("HMAC can take key of any size");
    mac.update(&message);
    let result = mac.finalize();
    let signature = result.into_bytes();

    // Format: aomi-{first 32 hex chars of signature}
    let hex: String = signature.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    format!("aomi-{}", hex)
}

fn api_key_to_json(row: &aomi_tools::db::ApiKey) -> Result<Value> {
    Ok(serde_json::json!({
        "id": row.id,
        "api_key": row.api_key,
        "label": row.label,
        "allowed_namespaces": row.allowed_namespaces,
        "is_active": row.is_active,
        "created_at": row.created_at,
    }))
}
