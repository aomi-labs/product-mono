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

        if update.label.is_some() || update.clear_label {
            updates += 1;
        }
        if update.allowed_namespaces.is_some() {
            updates += 1;
        }
        if update.is_active.is_some() {
            updates += 1;
        }

        if updates == 0 {
            bail!("no fields provided to update");
        }

        let namespaces_json = update
            .allowed_namespaces
            .map(|namespaces| serde_json::to_string(&namespaces))
            .transpose()?;

        let row = sqlx::query_as::<Any, ApiKey>(
            r#"
            UPDATE api_keys SET
                label = CASE WHEN $1 THEN NULL ELSE COALESCE($2, label) END,
                allowed_namespaces = COALESCE($3::jsonb, allowed_namespaces),
                is_active = COALESCE($4, is_active)
            WHERE api_key = $5
            RETURNING id, api_key, label, allowed_namespaces::TEXT as allowed_namespaces, is_active, created_at
            "#,
        )
        .bind(update.clear_label)
        .bind(update.label)
        .bind(namespaces_json)
        .bind(update.is_active)
        .bind(update.api_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }
}
