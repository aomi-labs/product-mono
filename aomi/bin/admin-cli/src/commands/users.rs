use anyhow::{Result, bail};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};

use crate::cli::{UserDeleteArgs, UserListArgs, UserUpdateArgs};
use crate::models::UserRow;
use crate::util::print_json;

pub async fn list_users(args: UserListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT public_key, username, created_at, namespaces FROM users ORDER BY created_at DESC",
    );

    if let Some(limit) = args.limit {
        query.push(" LIMIT ").push_bind(limit);
    }

    if let Some(offset) = args.offset {
        query.push(" OFFSET ").push_bind(offset);
    }

    let rows: Vec<UserRow> = query.build_query_as().fetch_all(pool).await?;
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

pub async fn update_user(args: UserUpdateArgs, pool: &sqlx::PgPool) -> Result<()> {
    if args.clear_username && args.username.is_some() {
        bail!("cannot set --username and --clear-username together");
    }

    let mut updates = 0;
    let mut query = QueryBuilder::<Postgres>::new("UPDATE users SET ");
    let mut separated = query.separated(", ");

    if let Some(ref username) = args.username {
        separated.push("username = ").push_bind_unseparated(username.clone());
        updates += 1;
    }

    if args.clear_username {
        separated.push("username = NULL");
        updates += 1;
    }

    if let Some(ref namespaces) = args.namespaces {
        let ns: Vec<String> = namespaces
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if ns.is_empty() {
            bail!("no valid namespaces provided");
        }
        separated.push("namespaces = ").push_bind_unseparated(ns);
        updates += 1;
    }

    if updates == 0 {
        bail!("no fields provided to update");
    }

    query.push(" WHERE public_key = ").push_bind(&args.public_key);
    query.push(" RETURNING public_key, username, created_at, namespaces");

    let row: UserRow = query.build_query_as().fetch_one(pool).await?;

    print_json(&serde_json::json!({
        "public_key": row.public_key,
        "username": row.username,
        "created_at": row.created_at,
        "namespaces": row.namespaces,
    }))?;
    Ok(())
}

pub async fn delete_user(args: UserDeleteArgs, pool: &sqlx::PgPool) -> Result<()> {
    let result = sqlx::query("DELETE FROM users WHERE public_key = $1")
        .bind(&args.public_key)
        .execute(pool)
        .await?;

    print_json(&serde_json::json!({
        "public_key": args.public_key,
        "deleted": result.rows_affected(),
    }))?;
    Ok(())
}
