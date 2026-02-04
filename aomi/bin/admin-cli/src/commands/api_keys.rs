use anyhow::{Context, Result, bail};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;

use crate::cli::{ApiKeyCreateArgs, ApiKeyListArgs, ApiKeyUpdateArgs};
use crate::util::print_json;
use aomi_tools::db::{ApiKeyStore, ApiKeyStoreApi, ApiKeyUpdate};

pub async fn create_api_key(args: ApiKeyCreateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let namespaces = normalize_namespaces(args.namespaces)?;
    let api_key = args.key.unwrap_or_else(generate_api_key);
    let store = ApiKeyStore::new(pool.clone());

    // Use create_api_key_multi for multiple namespaces (normalized schema)
    let rows = store
        .create_api_key_multi(api_key, args.label, namespaces)
        .await
        .context("failed to insert api key")?;

    // Output all created entries
    let json_rows = rows
        .iter()
        .map(api_key_to_json)
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
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

    let is_active = if args.active || args.inactive {
        Some(args.active)
    } else {
        None
    };

    let store = ApiKeyStore::new(pool.clone());

    // If namespace is provided, update specific entry; otherwise update all namespaces
    let rows = match args.namespace {
        Some(namespace) => {
            let update = ApiKeyUpdate {
                api_key: args.api_key,
                namespace,
                label: args.label,
                clear_label: args.clear_label,
                is_active,
            };
            vec![store.update_api_key(update).await?]
        }
        None => {
            // Update all namespaces for this API key
            store
                .update_api_key_all_namespaces(
                    &args.api_key,
                    args.label,
                    args.clear_label,
                    is_active,
                )
                .await?
        }
    };

    let json_rows = rows
        .iter()
        .map(api_key_to_json)
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
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

fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn api_key_to_json(row: &aomi_tools::db::ApiKey) -> Result<Value> {
    Ok(serde_json::json!({
        "id": row.id,
        "api_key": row.api_key,
        "label": row.label,
        "namespace": row.namespace,
        "is_active": row.is_active,
        "created_at": row.created_at,
    }))
}
