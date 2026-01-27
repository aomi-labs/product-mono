use anyhow::{Result, bail};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};

use crate::cli::{UserDeleteArgs, UserListArgs, UserUpdateArgs};
use crate::models::UserRow;
use crate::util::print_json;

pub async fn list_users(args: UserListArgs, pool: &sqlx::PgPool) -> Result<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT public_key, username, created_at FROM users ORDER BY created_at DESC",
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

    let row: UserRow = if let Some(username) = args.username {
        sqlx::query_as::<Postgres, UserRow>(
            "UPDATE users SET username = $1 WHERE public_key = $2 RETURNING public_key, username, created_at",
        )
        .bind(username)
        .bind(&args.public_key)
        .fetch_one(pool)
        .await?
    } else if args.clear_username {
        sqlx::query_as::<Postgres, UserRow>(
            "UPDATE users SET username = NULL WHERE public_key = $1 RETURNING public_key, username, created_at",
        )
        .bind(&args.public_key)
        .fetch_one(pool)
        .await?
    } else {
        bail!("no fields provided to update");
    };
    print_json(&serde_json::json!({
        "public_key": row.public_key,
        "username": row.username,
        "created_at": row.created_at,
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
