use anyhow::{Result, bail};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};

use crate::cli::{SessionDeleteArgs, SessionListArgs, SessionUpdateArgs};
use crate::models::SessionRow;
use crate::util::print_json;

pub async fn list_sessions(args: SessionListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, public_key, started_at, last_active_at, title, pending_transaction IS NOT NULL AS has_pending FROM sessions",
    );

    if let Some(public_key) = args.public_key {
        query.push(" WHERE public_key = ").push_bind(public_key);
    }

    query.push(" ORDER BY last_active_at DESC");

    if let Some(limit) = args.limit {
        query.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = args.offset {
        query.push(" OFFSET ").push_bind(offset);
    }

    let rows: Vec<SessionRow> = query.build_query_as().fetch_all(pool).await?;
    let json_rows = rows
        .iter()
        .map(|row| {
            Ok(serde_json::json!({
                "id": row.id,
                "public_key": row.public_key,
                "started_at": row.started_at,
                "last_active_at": row.last_active_at,
                "title": row.title,
                "has_pending": row.has_pending,
            }))
        })
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_session(args: SessionUpdateArgs, pool: &sqlx::PgPool) -> Result<()> {
    if args.clear_title && args.title.is_some() {
        bail!("cannot set --title and --clear-title together");
    }

    if args.clear_public_key && args.public_key.is_some() {
        bail!("cannot set --public-key and --clear-public-key together");
    }

    let mut updates = 0;
    let mut query = QueryBuilder::<Postgres>::new("UPDATE sessions SET ");
    let mut separated = query.separated(", ");

    if let Some(title) = args.title {
        separated.push("title = ").push_bind_unseparated(title);
        updates += 1;
    }

    if args.clear_title {
        separated.push("title = NULL");
        updates += 1;
    }

    if let Some(public_key) = args.public_key {
        separated
            .push("public_key = ")
            .push_bind_unseparated(public_key);
        updates += 1;
    }

    if args.clear_public_key {
        separated.push("public_key = NULL");
        updates += 1;
    }

    if updates == 0 {
        bail!("no fields provided to update");
    }

    query.push(" WHERE id = ").push_bind(args.id);
    query.push(
        " RETURNING id, public_key, started_at, last_active_at, title, pending_transaction IS NOT NULL AS has_pending",
    );

    let row: SessionRow = query.build_query_as().fetch_one(pool).await?;
    print_json(&serde_json::json!({
        "id": row.id,
        "public_key": row.public_key,
        "started_at": row.started_at,
        "last_active_at": row.last_active_at,
        "title": row.title,
        "has_pending": row.has_pending,
    }))?;
    Ok(())
}

pub async fn delete_session(args: SessionDeleteArgs, pool: &sqlx::PgPool) -> Result<()> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&args.id)
        .execute(pool)
        .await?;

    print_json(&serde_json::json!({
        "id": args.id,
        "deleted": result.rows_affected(),
    }))?;
    Ok(())
}
