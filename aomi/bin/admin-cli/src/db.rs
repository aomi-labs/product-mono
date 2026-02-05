use anyhow::{Context, Result};
use sqlx::{AnyPool, any::AnyPoolOptions};

pub fn resolve_database_url(cli_value: Option<String>) -> String {
    if let Some(url) = cli_value {
        return url;
    }

    let user = std::env::var("PGUSER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "postgres".to_string());
    let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("PGPORT").unwrap_or_else(|_| "5432".to_string());
    let database = std::env::var("PGDATABASE").unwrap_or_else(|_| "chatbot".to_string());
    format!("postgres://{user}@{host}:{port}/{database}")
}

pub async fn connect_database(database_url: &str) -> Result<AnyPool> {
    sqlx::any::install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .with_context(|| format!("failed to connect to database {database_url}"))?;
    Ok(pool)
}
