use anyhow::{Result, bail};
use serde_json::Value;

use crate::cli::{SessionDeleteArgs, SessionListArgs, SessionUpdateArgs};
use crate::util::print_json;
use aomi_tools::db::{SessionStore, SessionStoreApi};

pub async fn list_sessions(args: SessionListArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = SessionStore::new(pool.clone());
    let rows = store
        .list_sessions(args.public_key, args.limit, args.offset)
        .await?;
    let json_rows = rows
        .iter()
        .map(|row| {
            Ok(serde_json::json!({
                "id": row.id,
                "public_key": row.public_key,
                "started_at": row.started_at,
                "last_active_at": row.last_active_at,
                "title": row.title,
                "has_pending": row.pending_transaction.is_some(),
            }))
        })
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_session(args: SessionUpdateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    if args.clear_title && args.title.is_some() {
        bail!("cannot set --title and --clear-title together");
    }

    if args.clear_public_key && args.public_key.is_some() {
        bail!("cannot set --public-key and --clear-public-key together");
    }

    let store = SessionStore::new(pool.clone());
    let mut updates = 0;

    if let Some(title) = args.title {
        store.set_session_title(&args.id, Some(title)).await?;
        updates += 1;
    }

    if args.clear_title {
        store.set_session_title(&args.id, None).await?;
        updates += 1;
    }

    if let Some(public_key) = args.public_key {
        store
            .update_session_public_key(&args.id, Some(public_key))
            .await?;
        updates += 1;
    }

    if args.clear_public_key {
        store.update_session_public_key(&args.id, None).await?;
        updates += 1;
    }

    if updates == 0 {
        bail!("no fields provided to update");
    }

    let row = store
        .get_session(&args.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("session not found"))?;
    print_json(&serde_json::json!({
        "id": row.id,
        "public_key": row.public_key,
        "started_at": row.started_at,
        "last_active_at": row.last_active_at,
        "title": row.title,
        "has_pending": row.pending_transaction.is_some(),
    }))?;
    Ok(())
}

pub async fn delete_session(args: SessionDeleteArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = SessionStore::new(pool.clone());
    let deleted = store.delete_session_only(&args.id).await?;

    print_json(&serde_json::json!({
        "id": args.id,
        "deleted": deleted,
    }))?;
    Ok(())
}
