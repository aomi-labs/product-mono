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

use aomi_backend::{is_not_default, DEFAULT_NAMESPACE};

// ============================================================================
// Constants
// ============================================================================

pub const API_KEY_HEADER: &str = "X-API-Key";
pub const SESSION_ID_HEADER: &str = "X-Session-Id";

/// Paths that require a session ID header.
const SESSION_REQUIRED_PATHS: &[&str] = &[
    "/api/chat",
    "/api/state",
    "/api/interrupt",
    "/api/updates",
    "/api/system",
    "/api/events",
    "/api/memory-mode",
];

/// Path prefixes where session ID is required when followed by a non-empty suffix.
const SESSION_REQUIRED_PREFIXES: &[&str] = &["/api/sessions/", "/api/db/sessions/"];

/// Paths where API key is validated for non-default namespaces.
const API_KEY_CHECKED_PATHS: &[&str] = &["/api/chat"];

// ============================================================================
// Types
// ============================================================================

#[derive(Clone)]
pub struct ApiAuth {
    pool: AnyPool,
}

#[derive(Clone)]
pub struct AuthorizedKey {
    allowed_namespaces: HashSet<String>,
}

#[derive(Clone, Debug)]
pub struct SessionId(pub String);

// ============================================================================
// ApiAuth
// ============================================================================

impl ApiAuth {
    pub async fn from_db(pool: AnyPool) -> Result<Arc<Self>> {
        Ok(Arc::new(Self { pool }))
    }

    /// Validate an API key and return the authorized key if valid.
    pub async fn authorize_key(&self, key: &str) -> Result<Option<AuthorizedKey>> {
        let row = sqlx::query(
            "SELECT CAST(allowed_namespaces AS TEXT) AS allowed_namespaces \
             FROM api_keys WHERE api_key = $1 AND is_active = TRUE",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to query api_keys table")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let raw: String = row
            .try_get("allowed_namespaces")
            .context("Failed to read allowed_namespaces")?;
        let namespaces: Vec<String> =
            serde_json::from_str(&raw).context("Invalid allowed_namespaces JSON")?;

        Ok(Some(AuthorizedKey::new(namespaces)))
    }

    /// Returns true if middleware should be skipped for this request.
    fn should_skip(&self, req: &Request<Body>) -> bool {
        req.method() == Method::OPTIONS || !req.uri().path().starts_with("/api/")
    }

    /// Returns true if the request requires a session ID header.
    fn requires_session_id(&self, req: &Request<Body>) -> bool {
        let path = req.uri().path();

        if SESSION_REQUIRED_PATHS.contains(&path) {
            return true;
        }

        // POST /api/sessions requires session ID (frontend generates it)
        if path == "/api/sessions" && req.method() == Method::POST {
            return true;
        }

        // Prefix matches with non-empty suffix (e.g., /api/sessions/{id})
        SESSION_REQUIRED_PREFIXES
            .iter()
            .any(|prefix| path.strip_prefix(prefix).is_some_and(|s| !s.is_empty()))
    }

    /// Returns true if the request requires API key validation.
    fn requires_api_key(&self, req: &Request<Body>) -> bool {
        let path = req.uri().path();

        if !API_KEY_CHECKED_PATHS.contains(&path) {
            return false;
        }

        let namespace = self.extract_namespace(req);
        is_not_default(&namespace)
    }

    /// Extract namespace from query parameters (namespace or chatbot).
    fn extract_namespace(&self, req: &Request<Body>) -> String {
        let query = req.uri().query().unwrap_or("");
        query_param(query, "namespace")
            .or_else(|| query_param(query, "chatbot"))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(DEFAULT_NAMESPACE)
            .to_string()
    }

    /// Extract session ID from request headers.
    fn extract_session_id(&self, req: &Request<Body>) -> Option<String> {
        req.headers()
            .get(SESSION_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    }

    /// Extract API key from request headers.
    fn extract_api_key<'a>(&self, req: &'a Request<Body>) -> Option<&'a str> {
        req.headers()
            .get(API_KEY_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|k| !k.is_empty())
    }
}

// ============================================================================
// AuthorizedKey
// ============================================================================

impl AuthorizedKey {
    fn new(namespaces: Vec<String>) -> Self {
        let allowed_namespaces = namespaces
            .into_iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Self { allowed_namespaces }
    }

    pub fn allows_namespace(&self, namespace: &str) -> bool {
        self.allowed_namespaces.contains(&namespace.to_lowercase())
    }
}

// ============================================================================
// Middleware
// ============================================================================

pub async fn api_key_middleware(
    State(auth): State<Arc<ApiAuth>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if auth.should_skip(&req) {
        return Ok(next.run(req).await);
    }

    if auth.requires_session_id(&req) {
        let session_id = auth.extract_session_id(&req).ok_or(StatusCode::BAD_REQUEST)?;
        req.extensions_mut().insert(SessionId(session_id));
    }

    if auth.requires_api_key(&req) {
        let key = auth.extract_api_key(&req).ok_or(StatusCode::UNAUTHORIZED)?;

        let authorized = auth
            .authorize_key(key)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::FORBIDDEN)?;

        req.extensions_mut().insert(authorized);
    }

    Ok(next.run(req).await)
}

// ============================================================================
// Helpers
// ============================================================================

/// Extract a query parameter value by key.
fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let pair_key = parts.next()?.trim();
        if pair_key == key {
            Some(parts.next().unwrap_or(""))
        } else {
            None
        }
    })
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

    async fn state_handler(Extension(SessionId(session_id)): Extension<SessionId>) -> StatusCode {
        if session_id == "session-1" {
            StatusCode::OK
        } else {
            StatusCode::BAD_REQUEST
        }
    }

    async fn chat_handler(
        api_key: Option<Extension<AuthorizedKey>>,
        Extension(SessionId(_session_id)): Extension<SessionId>,
        Query(params): Query<HashMap<String, String>>,
    ) -> StatusCode {
        let namespace = params
            .get("namespace")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_NAMESPACE);
        if is_not_default(namespace) {
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
            .route("/api/state", get(state_handler))
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
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/state")
                    .header(SESSION_ID_HEADER, "session-1")
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
                    .uri("/api/state")
                    .header(SESSION_ID_HEADER, "session-1")
                    .header(API_KEY_HEADER, "invalid-key")
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
                    .uri("/api/state")
                    .header(SESSION_ID_HEADER, "session-1")
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
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat?namespace=default")
                    .header(SESSION_ID_HEADER, "session-1")
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
                    .header(SESSION_ID_HEADER, "session-1")
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
                    .header(SESSION_ID_HEADER, "session-1")
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
                    .header(SESSION_ID_HEADER, "session-1")
                    .header(API_KEY_HEADER, "valid-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
