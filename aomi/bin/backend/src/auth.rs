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
pub const SESSION_ID_HEADER: &str = "X-Session-Id";
pub const DEFAULT_NAMESPACE: &str = "default";

const API_PATH_PREFIX: &str = "/api/";

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

pub fn is_not_default(namespace: &str) -> bool {
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
    if should_skip_middleware(&req) {
        return Ok(next.run(req).await);
    }

    if requires_session_id(&req) {
        let session_id = req
            .headers()
            .get(SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let session_id = match session_id {
            Some(value) => value,
            None => return Err(StatusCode::BAD_REQUEST),
        };
        req.extensions_mut().insert(SessionId(session_id));
    }

    if requires_api_key(&req) {
        let header = req
            .headers()
            .get(API_KEY_HEADER)
            .and_then(|value| value.to_str().ok());
        let key = match header {
            Some(key) if !key.trim().is_empty() => key.trim(),
            _ => return Err(StatusCode::UNAUTHORIZED),
        };

        let authorized = match auth.authorize_key(key).await {
            Ok(Some(value)) => value,
            Ok(None) => return Err(StatusCode::FORBIDDEN),
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        req.extensions_mut().insert(authorized);
    }

    Ok(next.run(req).await)
}

fn should_skip_middleware(req: &Request<Body>) -> bool {
    if req.method() == Method::OPTIONS {
        return true;
    }

    let path = req.uri().path();
    !path.starts_with(API_PATH_PREFIX)
}

fn requires_api_key(req: &Request<Body>) -> bool {
    if req.uri().path() != "/api/chat" {
        return false;
    }

    let namespace = chat_namespace(req);
    is_not_default(&namespace)
}

fn requires_session_id(req: &Request<Body>) -> bool {
    let path = req.uri().path();
    if matches!(
        path,
        "/api/chat"
            | "/api/state"
            | "/api/interrupt"
            | "/api/updates"
            | "/api/system"
            | "/api/events"
            | "/api/memory-mode"
    ) {
        return true;
    }

    if let Some(suffix) = path.strip_prefix("/api/sessions/") {
        return !suffix.is_empty();
    }

    if let Some(suffix) = path.strip_prefix("/api/db/sessions/") {
        return !suffix.is_empty();
    }

    false
}

fn chat_namespace(req: &Request<Body>) -> String {
    let query = req.uri().query().unwrap_or("");
    let namespace = query_param(query, "namespace")
        .or_else(|| query_param(query, "chatbot"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_NAMESPACE);
    namespace.to_string()
}

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
