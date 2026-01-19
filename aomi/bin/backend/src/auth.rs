use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::{AnyPool, Row};
use std::{collections::HashSet, sync::Arc};

pub const API_KEY_HEADER: &str = "X-API-Key";
pub const DEFAULT_NAMESPACE: &str = "default";

const API_PATH_PREFIX: &str = "/api/";
const PUBLIC_API_PATH: &str = "/api/updates";
const PUBLIC_API_PATH_PREFIX: &str = "/api/updates/";

#[derive(Clone)]
pub struct ApiAuth {
    pool: AnyPool,
}

#[derive(Clone)]
pub struct AuthorizedKey {
    allowed_namespaces: HashSet<String>,
}

pub fn requires_namespace_auth(namespace: &str) -> bool {
    !namespace.eq_ignore_ascii_case(DEFAULT_NAMESPACE)
}

impl ApiAuth {
    pub async fn from_db(pool: AnyPool) -> Result<Arc<Self>> {
        Ok(Arc::new(Self { pool }))
    }

    pub async fn authorize_key(&self, key: &str) -> Result<Option<AuthorizedKey>> {
        let row = sqlx::query(
            "SELECT CAST(allowed_namespaces AS TEXT) AS allowed_namespaces FROM api_keys WHERE api_key = $1 AND is_active = TRUE",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to query api_keys table")?;
        let Some(row) = row else {
            return Ok(None);
        };
        let allowed_namespaces_raw: String = row
            .try_get("allowed_namespaces")
            .context("Failed to read allowed_namespaces")?;
        let allowed_namespaces_vec: Vec<String> = serde_json::from_str(&allowed_namespaces_raw)
            .context("Invalid allowed_namespaces JSON")?;
        let allowed_namespaces = normalize_namespaces(allowed_namespaces_vec);
        Ok(Some(AuthorizedKey { allowed_namespaces }))
    }
}

impl AuthorizedKey {
    pub fn allows_namespace(&self, namespace: &str) -> bool {
        self.allowed_namespaces.contains(&namespace.to_lowercase())
    }
}

pub async fn api_key_middleware(
    State(auth): State<Arc<ApiAuth>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if should_skip_auth(&req) {
        return Ok(next.run(req).await);
    }

    let header = req
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    let key = match header {
        Some(key) if !key.trim().is_empty() => Some(key.trim()),
        _ => None,
    };
    if key.is_none() && req.uri().path() == "/api/chat" {
        return Ok(next.run(req).await);
    }
    let key = match key {
        Some(key) => key,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let authorized = match auth.authorize_key(key).await {
        Ok(Some(value)) => value,
        Ok(None) => return Err(StatusCode::FORBIDDEN),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    req.extensions_mut().insert(authorized);
    Ok(next.run(req).await)
}

fn should_skip_auth(req: &Request<Body>) -> bool {
    if req.method() == Method::OPTIONS {
        return true;
    }

    let path = req.uri().path();
    if !path.starts_with(API_PATH_PREFIX) {
        return true;
    }

    path == PUBLIC_API_PATH || path.starts_with(PUBLIC_API_PATH_PREFIX)
}

fn normalize_namespaces(entries: Vec<String>) -> HashSet<String> {
    let mut allowed = HashSet::new();
    for entry in entries {
        let entry = entry.trim();
        if !entry.is_empty() {
            allowed.insert(entry.to_lowercase());
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Query,
        http::{Request, StatusCode},
        routing::{get, post},
        Extension, Router,
    };
    use sqlx::{any::AnyPoolOptions, Any};
    use std::collections::HashMap;
    use tower::util::ServiceExt;

    async fn setup_pool() -> AnyPool {
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to open sqlite memory db");

        sqlx::query::<Any>(
            r#"
            CREATE TABLE api_keys (
                api_key TEXT PRIMARY KEY,
                allowed_namespaces TEXT NOT NULL,
                is_active BOOLEAN NOT NULL DEFAULT TRUE
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create api_keys table");

        pool
    }

    async fn insert_key(pool: &AnyPool, api_key: &str, allowed_namespaces: &str, is_active: bool) {
        sqlx::query::<Any>(
            "INSERT INTO api_keys (api_key, allowed_namespaces, is_active) VALUES ($1, $2, $3)",
        )
        .bind(api_key)
        .bind(allowed_namespaces)
        .bind(is_active)
        .execute(pool)
        .await
        .expect("failed to insert api key");
    }

    #[tokio::test]
    async fn authorize_key_reads_allowed_namespaces() {
        let pool = setup_pool().await;
        insert_key(&pool, "key-1", r#"["DEFAULT","L2BEAT"]"#, true).await;

        let auth = ApiAuth::from_db(pool).await.expect("auth init failed");
        let key = auth
            .authorize_key("key-1")
            .await
            .expect("authorize failed")
            .expect("missing key");

        assert!(key.allows_namespace("default"));
        assert!(key.allows_namespace("l2beat"));
        assert!(!key.allows_namespace("other"));
    }

    async fn ok_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn chat_handler(
        api_key: Option<Extension<AuthorizedKey>>,
        Query(params): Query<HashMap<String, String>>,
    ) -> StatusCode {
        let namespace = params
            .get("namespace")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_NAMESPACE);
        if requires_namespace_auth(namespace) {
            let Extension(api_key) = match api_key {
                Some(value) => value,
                None => return StatusCode::UNAUTHORIZED,
            };
            if !api_key.allows_namespace(namespace) {
                return StatusCode::FORBIDDEN;
            }
        }
        StatusCode::OK
    }

    #[tokio::test]
    async fn middleware_enforces_api_key_on_protected_routes() {
        let pool = setup_pool().await;
        insert_key(&pool, "valid-key", r#"["l2beat"]"#, true).await;
        insert_key(&pool, "default-key", r#"["default"]"#, true).await;

        let auth = ApiAuth::from_db(pool).await.expect("auth init failed");
        let app = Router::new()
            .route("/api/state", get(ok_handler))
            .route("/api/chat", post(chat_handler))
            .layer(axum::middleware::from_fn_with_state(
                auth,
                api_key_middleware,
            ));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/state")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/state")
                    .header(API_KEY_HEADER, "invalid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/state")
                    .header(API_KEY_HEADER, "valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat?namespace=default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat?namespace=l2beat")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat?namespace=l2beat")
                    .header(API_KEY_HEADER, "default-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat?namespace=l2beat")
                    .header(API_KEY_HEADER, "valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
