use anyhow::{bail, Context, Result};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};

use crate::cli::{ApiKeyCreateArgs, ApiKeyListArgs, ApiKeyUpdateArgs};
use crate::models::ApiKeyRow;
use crate::util::print_json;

pub async fn create_api_key(args: ApiKeyCreateArgs, pool: &sqlx::PgPool) -> Result<()> {
    let chatbots = parse_chatbots(&args.chatbots)?;
    let chatbots_json = serde_json::to_string(&chatbots)?;
    let api_key = args.key.unwrap_or_else(generate_api_key);

    let row: ApiKeyRow = sqlx::query_as::<Postgres, ApiKeyRow>(
        "INSERT INTO api_keys (api_key, label, allowed_chatbots)\
         VALUES ($1, $2, $3::jsonb)\
         RETURNING id, api_key, label, allowed_chatbots::TEXT as allowed_chatbots, is_active, created_at",
    )
    .bind(&api_key)
    .bind(args.label)
    .bind(chatbots_json)
    .fetch_one(pool)
    .await
    .context("failed to insert api key")?;

    print_json(&api_key_to_json(&row)?)?;
    Ok(())
}

pub async fn list_api_keys(args: ApiKeyListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, api_key, label, allowed_chatbots::TEXT as allowed_chatbots, is_active, created_at FROM api_keys",
    );

    if args.active_only {
        query.push(" WHERE is_active = TRUE");
    }

    query.push(" ORDER BY id");

    if let Some(limit) = args.limit {
        query.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = args.offset {
        query.push(" OFFSET ").push_bind(offset);
    }

    let rows: Vec<ApiKeyRow> = query.build_query_as().fetch_all(pool).await?;
    let json_rows = rows
        .iter()
        .map(api_key_to_json)
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_api_key(args: ApiKeyUpdateArgs, pool: &sqlx::PgPool) -> Result<()> {
    if args.clear_label && args.label.is_some() {
        bail!("cannot set --label and --clear-label together");
    }

    if args.active && args.inactive {
        bail!("cannot set --active and --inactive together");
    }

    let mut updates = 0;
    let mut query = QueryBuilder::<Postgres>::new("UPDATE api_keys SET ");
    let mut separated = query.separated(", ");

    if let Some(label) = args.label {
        separated.push("label = ").push_bind_unseparated(label);
        updates += 1;
    }

    if args.clear_label {
        separated.push("label = NULL");
        updates += 1;
    }

    if let Some(chatbots) = args.chatbots {
        let chatbots = parse_chatbots(&chatbots)?;
        let chatbots_json = serde_json::to_string(&chatbots)?;
        separated
            .push("allowed_chatbots = ")
            .push_bind_unseparated(chatbots_json)
            .push_unseparated("::jsonb");
        updates += 1;
    }

    if args.active || args.inactive {
        separated
            .push("is_active = ")
            .push_bind_unseparated(args.active);
        updates += 1;
    }

    if updates == 0 {
        bail!("no fields provided to update");
    }

    query.push(" WHERE api_key = ").push_bind(args.api_key);
    query.push(
        " RETURNING id, api_key, label, allowed_chatbots::TEXT as allowed_chatbots, is_active, created_at",
    );

    let row: ApiKeyRow = query.build_query_as().fetch_one(pool).await?;
    print_json(&api_key_to_json(&row)?)?;
    Ok(())
}

fn parse_chatbots(raw: &str) -> Result<Vec<String>> {
    let values: Vec<String> = raw
        .split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect();

    if values.is_empty() {
        bail!("no chatbots provided after parsing");
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

fn api_key_to_json(row: &ApiKeyRow) -> Result<Value> {
    let allowed_chatbots: Value = serde_json::from_str(&row.allowed_chatbots).with_context(|| {
        format!(
            "invalid allowed_chatbots JSON for key {}",
            row.api_key
        )
    })?;

    Ok(serde_json::json!({
        "id": row.id,
        "api_key": row.api_key,
        "label": row.label,
        "allowed_chatbots": allowed_chatbots,
        "is_active": row.is_active,
        "created_at": row.created_at,
    }))
}
