use anyhow::{Context, Result, bail};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};
use std::collections::BTreeMap;

use crate::cli::{ApiKeyCreateArgs, ApiKeyListArgs, ApiKeyUpdateArgs};
use crate::models::ApiKeyRow;
use crate::util::print_json;

pub async fn create_api_key(args: ApiKeyCreateArgs, pool: &sqlx::PgPool) -> Result<()> {
    let namespaces = normalize_namespaces(&args.namespaces)?;
    let api_key = args.key.unwrap_or_else(generate_api_key);

    // Insert one row per namespace
    let mut inserted_rows: Vec<ApiKeyRow> = Vec::new();
    for namespace in &namespaces {
        let row: ApiKeyRow = sqlx::query_as::<Postgres, ApiKeyRow>(
            "INSERT INTO api_keys (api_key, label, namespace) \
             VALUES ($1, $2, $3) \
             RETURNING id, api_key, label, namespace, is_active, created_at",
        )
        .bind(&api_key)
        .bind(&args.label)
        .bind(namespace)
        .fetch_one(pool)
        .await
        .context("failed to insert api key")?;
        inserted_rows.push(row);
    }

    print_json(&rows_to_json(&inserted_rows))?;
    Ok(())
}

pub async fn list_api_keys(args: ApiKeyListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, api_key, label, namespace, is_active, created_at FROM api_keys",
    );

    if args.active_only {
        query.push(" WHERE is_active = TRUE");
    }

    query.push(" ORDER BY api_key, id");

    if let Some(limit) = args.limit {
        query.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = args.offset {
        query.push(" OFFSET ").push_bind(offset);
    }

    let rows: Vec<ApiKeyRow> = query.build_query_as().fetch_all(pool).await?;
    print_json(&Value::from(group_rows_by_key(&rows)))?;
    Ok(())
}

pub async fn update_api_key(args: ApiKeyUpdateArgs, pool: &sqlx::PgPool) -> Result<()> {
    if args.clear_label && args.label.is_some() {
        bail!("cannot set --label and --clear-label together");
    }

    if args.active && args.inactive {
        bail!("cannot set --active and --inactive together");
    }

    // Handle namespace updates (requires delete + insert)
    if let Some(ref ns) = args.namespaces {
        let new_namespaces = normalize_namespaces(&Some(ns.clone()))?;

        let existing: ApiKeyRow = sqlx::query_as::<Postgres, ApiKeyRow>(
            "SELECT id, api_key, label, namespace, is_active, created_at \
             FROM api_keys WHERE api_key = $1 LIMIT 1",
        )
        .bind(&args.api_key)
        .fetch_optional(pool)
        .await
        .context("failed to fetch existing api key")?
        .context("api key not found")?;

        sqlx::query("DELETE FROM api_keys WHERE api_key = $1")
            .bind(&args.api_key)
            .execute(pool)
            .await
            .context("failed to delete existing namespaces")?;

        let label = args.label.clone().or(existing.label);
        let is_active = match (args.active, args.inactive) {
            (true, _) => true,
            (_, true) => false,
            _ => existing.is_active,
        };

        for namespace in &new_namespaces {
            sqlx::query(
                "INSERT INTO api_keys (api_key, label, namespace, is_active, created_at) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&args.api_key)
            .bind(&label)
            .bind(namespace)
            .bind(is_active)
            .bind(existing.created_at)
            .execute(pool)
            .await
            .context("failed to insert namespace")?;
        }
    } else {
        // Update label and/or is_active without changing namespaces
        let mut updates = 0;
        let mut query = QueryBuilder::<Postgres>::new("UPDATE api_keys SET ");
        let mut separated = query.separated(", ");

        if let Some(ref label) = args.label {
            separated.push("label = ").push_bind_unseparated(label.clone());
            updates += 1;
        }

        if args.clear_label {
            separated.push("label = NULL");
            updates += 1;
        }

        if args.active || args.inactive {
            separated.push("is_active = ").push_bind_unseparated(args.active);
            updates += 1;
        }

        if updates == 0 {
            bail!("no fields provided to update");
        }

        query.push(" WHERE api_key = ").push_bind(&args.api_key);
        query.build().execute(pool).await?;
    }

    let rows: Vec<ApiKeyRow> = sqlx::query_as::<Postgres, ApiKeyRow>(
        "SELECT id, api_key, label, namespace, is_active, created_at \
         FROM api_keys WHERE api_key = $1 ORDER BY id",
    )
    .bind(&args.api_key)
    .fetch_all(pool)
    .await?;

    print_json(&rows_to_json(&rows))?;
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

fn normalize_namespaces(namespaces: &Option<Vec<String>>) -> Result<Vec<String>> {
    let ns = namespaces.as_ref().context("namespaces required")?;
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
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn rows_to_json(rows: &[ApiKeyRow]) -> Value {
    if rows.is_empty() {
        return Value::Null;
    }

    let first = &rows[0];
    let namespaces: Vec<&str> = rows.iter().map(|r| r.namespace.as_str()).collect();

    serde_json::json!({
        "api_key": first.api_key,
        "label": first.label,
        "namespaces": namespaces,
        "is_active": first.is_active,
        "created_at": first.created_at,
    })
}

fn group_rows_by_key(rows: &[ApiKeyRow]) -> Vec<Value> {
    let mut grouped: BTreeMap<String, Vec<&ApiKeyRow>> = BTreeMap::new();
    for row in rows {
        grouped.entry(row.api_key.clone()).or_default().push(row);
    }

    grouped
        .into_values()
        .map(|group| {
            let first = group[0];
            let namespaces: Vec<&str> = group.iter().map(|r| r.namespace.as_str()).collect();
            serde_json::json!({
                "api_key": first.api_key,
                "label": first.label,
                "namespaces": namespaces,
                "is_active": first.is_active,
                "created_at": first.created_at,
            })
        })
        .collect()
}
