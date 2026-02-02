use anyhow::{Result, bail};
use serde_json::Value;

use crate::cli::{UserDeleteArgs, UserListArgs, UserUpdateArgs};
use crate::util::print_json;
use aomi_tools::db::{SessionStore, SessionStoreApi};

pub async fn list_users(args: UserListArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = SessionStore::new(pool.clone());
    let rows = store.list_users(args.limit, args.offset).await?;
    let json_rows = rows
        .iter()
        .map(|row| {
            Ok(serde_json::json!({
                "public_key": row.public_key,
                "username": row.username,
                "created_at": row.created_at,
            }))
        })
        .collect::<Result<Vec<Value>>>()?;

    print_json(&Value::from(json_rows))?;
    Ok(())
}

pub async fn update_user(args: UserUpdateArgs, pool: &sqlx::AnyPool) -> Result<()> {
    if args.clear_username && args.username.is_some() {
        bail!("cannot set --username and --clear-username together");
    }

    let store = SessionStore::new(pool.clone());
    if let Some(username) = args.username {
        store
            .update_user_username(&args.public_key, Some(username))
            .await?;
    } else if args.clear_username {
        store.update_user_username(&args.public_key, None).await?;
    } else {
        bail!("no fields provided to update");
    };

    let row = store
        .get_user(&args.public_key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found"))?;
    print_json(&serde_json::json!({
        "public_key": row.public_key,
        "username": row.username,
        "created_at": row.created_at,
    }))?;
    Ok(())
}

pub async fn delete_user(args: UserDeleteArgs, pool: &sqlx::AnyPool) -> Result<()> {
    let store = SessionStore::new(pool.clone());
    let deleted = store.delete_user(&args.public_key).await?;

    print_json(&serde_json::json!({
        "public_key": args.public_key,
        "deleted": deleted,
    }))?;
    Ok(())
}
