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
    async fn create_api_key(
        &self,
        api_key: String,
        label: Option<String>,
        allowed_namespaces: Vec<String>,
    ) -> Result<ApiKey> {
        let allowed_namespaces_json = serde_json::to_string(&allowed_namespaces)?;
        let query = "INSERT INTO api_keys (api_key, label, allowed_namespaces)\
                     VALUES ($1, $2, $3::jsonb)\
                     RETURNING id, api_key, label, allowed_namespaces::TEXT as allowed_namespaces, is_active, created_at";

        let row = sqlx::query_as::<Any, ApiKey>(query)
            .bind(api_key)
            .bind(label)
            .bind(allowed_namespaces_json)
            .fetch_one(&self.pool)
            .await?;

        Ok(row)
    }

    async fn list_api_keys(
        &self,
        active_only: bool,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ApiKey>> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, api_key, label, allowed_namespaces::TEXT as allowed_namespaces, is_active, created_at FROM api_keys",
        );

        if active_only {
            query.push(" WHERE is_active = TRUE");
        }

        query.push(" ORDER BY id");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let rows: Vec<ApiKey> = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(rows)
    }

    async fn update_api_key(&self, update: ApiKeyUpdate) -> Result<ApiKey> {
        if update.clear_label && update.label.is_some() {
            bail!("cannot set label and clear_label together");
        }

        let mut updates = 0;
        let mut query = QueryBuilder::<Any>::new("UPDATE api_keys SET ");
        let mut separated = query.separated(", ");

        if let Some(label) = update.label {
            separated.push("label = ").push_bind_unseparated(label);
            updates += 1;
        }

        if update.clear_label {
            separated.push("label = NULL");
            updates += 1;
        }

        if let Some(namespaces) = update.allowed_namespaces {
            let namespaces_json = serde_json::to_string(&namespaces)?;
            separated
                .push("allowed_namespaces = ")
                .push_bind_unseparated(namespaces_json)
                .push_unseparated("::jsonb");
            updates += 1;
        }

        if let Some(is_active) = update.is_active {
            separated
                .push("is_active = ")
                .push_bind_unseparated(is_active);
            updates += 1;
        }

        if updates == 0 {
            bail!("no fields provided to update");
        }

        query.push(" WHERE api_key = ").push_bind(update.api_key);
        query.push(
            " RETURNING id, api_key, label, allowed_namespaces::TEXT as allowed_namespaces, is_active, created_at",
        );

        let row: ApiKey = query.build_query_as().fetch_one(&self.pool).await?;
        Ok(row)
    }
}
