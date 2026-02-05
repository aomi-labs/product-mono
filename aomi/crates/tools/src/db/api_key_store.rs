use super::traits::ApiKeyStoreApi;
use super::{ApiKey, ApiKeyUpdate};
use anyhow::{Result, bail};
use async_trait::async_trait;
use sqlx::{Pool, QueryBuilder, any::Any};

#[derive(Clone, Debug)]
pub struct ApiKeyStore {
    pool: Pool<Any>,
}

impl ApiKeyStore {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyStoreApi for ApiKeyStore {
    /// Create a single API key + namespace entry.
    /// For multiple namespaces, call this multiple times or use create_api_key_multi.
    async fn create_api_key(
        &self,
        api_key: String,
        label: Option<String>,
        namespace: String,
    ) -> Result<ApiKey> {
        let query = "INSERT INTO api_keys (api_key, label, namespace) \
                     VALUES ($1, $2, $3) \
                     RETURNING id, api_key, label, namespace, is_active, created_at";

        let row = sqlx::query_as::<Any, ApiKey>(query)
            .bind(api_key)
            .bind(label)
            .bind(namespace)
            .fetch_one(&self.pool)
            .await?;

        Ok(row)
    }

    /// Create API key entries for multiple namespaces at once.
    /// Returns all created entries.
    async fn create_api_key_multi(
        &self,
        api_key: String,
        label: Option<String>,
        namespaces: Vec<String>,
    ) -> Result<Vec<ApiKey>> {
        if namespaces.is_empty() {
            bail!("at least one namespace is required");
        }

        let mut results = Vec::with_capacity(namespaces.len());

        for namespace in namespaces {
            let query = "INSERT INTO api_keys (api_key, label, namespace) \
                         VALUES ($1, $2, $3) \
                         ON CONFLICT (api_key, namespace) DO NOTHING \
                         RETURNING id, api_key, label, namespace, is_active, created_at";

            if let Ok(row) = sqlx::query_as::<Any, ApiKey>(query)
                .bind(&api_key)
                .bind(&label)
                .bind(namespace)
                .fetch_one(&self.pool)
                .await
            {
                results.push(row);
            }
        }

        Ok(results)
    }

    async fn list_api_keys(
        &self,
        active_only: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ApiKey>> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, api_key, label, namespace, is_active, created_at FROM api_keys",
        );

        if active_only {
            query.push(" WHERE is_active = TRUE");
        }

        query.push(" ORDER BY api_key, namespace");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let rows: Vec<ApiKey> = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(rows)
    }

    /// Get all namespaces for a specific API key
    async fn get_api_key_namespaces(&self, api_key: &str) -> Result<Vec<ApiKey>> {
        let query = "SELECT id, api_key, label, namespace, is_active, created_at \
                     FROM api_keys WHERE api_key = $1 ORDER BY namespace";

        let rows = sqlx::query_as::<Any, ApiKey>(query)
            .bind(api_key)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows)
    }

    async fn update_api_key(&self, update: ApiKeyUpdate) -> Result<ApiKey> {
        if update.clear_label && update.label.is_some() {
            bail!("cannot set label and clear_label together");
        }

        let mut updates = 0;

        if update.label.is_some() || update.clear_label {
            updates += 1;
        }
        if update.is_active.is_some() {
            updates += 1;
        }

        if updates == 0 {
            bail!("no fields provided to update");
        }

        let row = sqlx::query_as::<Any, ApiKey>(
            r#"
            UPDATE api_keys SET
                label = CASE WHEN $1 THEN NULL ELSE COALESCE($2, label) END,
                is_active = COALESCE($3, is_active)
            WHERE api_key = $4 AND namespace = $5
            RETURNING id, api_key, label, namespace, is_active, created_at
            "#,
        )
        .bind(update.clear_label)
        .bind(update.label)
        .bind(update.is_active)
        .bind(update.api_key)
        .bind(update.namespace)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    /// Update all entries for an API key (across all namespaces)
    async fn update_api_key_all_namespaces(
        &self,
        api_key: &str,
        label: Option<String>,
        clear_label: bool,
        is_active: Option<bool>,
    ) -> Result<Vec<ApiKey>> {
        if clear_label && label.is_some() {
            bail!("cannot set label and clear_label together");
        }

        let rows = sqlx::query_as::<Any, ApiKey>(
            r#"
            UPDATE api_keys SET
                label = CASE WHEN $1 THEN NULL ELSE COALESCE($2, label) END,
                is_active = COALESCE($3, is_active)
            WHERE api_key = $4
            RETURNING id, api_key, label, namespace, is_active, created_at
            "#,
        )
        .bind(clear_label)
        .bind(label)
        .bind(is_active)
        .bind(api_key)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Delete a specific API key + namespace entry
    async fn delete_api_key(&self, api_key: &str, namespace: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE api_key = $1 AND namespace = $2")
            .bind(api_key)
            .bind(namespace)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all entries for an API key
    async fn delete_api_key_all(&self, api_key: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM api_keys WHERE api_key = $1")
            .bind(api_key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}
