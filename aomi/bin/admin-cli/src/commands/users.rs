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
                "namespaces": row.namespaces,
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

    // Handle username update
    if let Some(username) = args.username.clone() {
        store
            .update_user_username(&args.public_key, Some(username))
            .await?;
    } else if args.clear_username {
        store.update_user_username(&args.public_key, None).await?;
    }

    // Handle namespaces update
    if let Some(ref namespaces) = args.namespaces {
        let ns: Vec<String> = namespaces
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if ns.is_empty() {
            bail!("no valid namespaces provided");
        }
        store.update_user_namespaces(&args.public_key, ns).await?;
    }

    // Check if any update was made
    if args.username.is_none() && !args.clear_username && args.namespaces.is_none() {
        bail!("no fields provided to update");
    }

    let row = store
        .get_user(&args.public_key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user not found"))?;
    print_json(&serde_json::json!({
        "public_key": row.public_key,
        "username": row.username,
        "created_at": row.created_at,
        "namespaces": row.namespaces,
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
