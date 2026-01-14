use anyhow::{bail, Context, Result};
use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub const API_KEY_HEADER: &str = "X-API-Key";
pub const DEFAULT_CHATBOT: &str = "default";

const API_PATH_PREFIX: &str = "/api/";
const PUBLIC_API_PATH: &str = "/api/updates";
const PUBLIC_API_PATH_PREFIX: &str = "/api/updates/";

#[derive(Clone)]
pub struct ApiAuth {
    keys: HashMap<String, ApiKeyPolicy>,
}

#[derive(Clone)]
pub struct AuthorizedKey {
    scopes: ChatbotScopes,
}

#[derive(Clone)]
struct ApiKeyPolicy {
    scopes: ChatbotScopes,
}

#[derive(Clone)]
enum ChatbotScopes {
    All,
    Limited(HashSet<String>),
}

impl ApiAuth {
    pub fn from_env() -> Result<Arc<Self>> {
        let raw = std::env::var("BACKEND_API_KEYS")
            .context("BACKEND_API_KEYS must be set (comma-separated API keys)")?;
        let keys = parse_keys(&raw)?;
        if keys.is_empty() {
            bail!("BACKEND_API_KEYS must include at least one API key");
        }
        Ok(Arc::new(Self { keys }))
    }

    pub fn authorize_key(&self, key: &str) -> Option<AuthorizedKey> {
        self.keys
            .get(key)
            .map(|policy| AuthorizedKey { scopes: policy.scopes.clone() })
    }
}

impl AuthorizedKey {
    pub fn allows_chatbot(&self, chatbot: &str) -> bool {
        match &self.scopes {
            ChatbotScopes::All => true,
            ChatbotScopes::Limited(scopes) => scopes.contains(chatbot),
        }
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
        Some(key) if !key.trim().is_empty() => key.trim(),
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    let authorized = match auth.authorize_key(key) {
        Some(value) => value,
        None => return Err(StatusCode::FORBIDDEN),
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

fn parse_keys(raw: &str) -> Result<HashMap<String, ApiKeyPolicy>> {
    let mut keys = HashMap::new();

    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let (key, scope_raw) = match entry.split_once(':') {
            Some((left, right)) => (left.trim(), right.trim()),
            None => (entry, "*"),
        };

        if key.is_empty() {
            bail!("BACKEND_API_KEYS contains an empty key entry");
        }

        let scopes = if scope_raw.is_empty() || scope_raw == "*" {
            ChatbotScopes::All
        } else {
            let mut allowed = HashSet::new();
            for scope in scope_raw.split('|') {
                let scope = scope.trim();
                if !scope.is_empty() {
                    allowed.insert(scope.to_string());
                }
            }
            if allowed.is_empty() {
                bail!("API key '{}' has no valid chatbot scopes", key);
            }
            ChatbotScopes::Limited(allowed)
        };

        if keys.insert(key.to_string(), ApiKeyPolicy { scopes }).is_some() {
            bail!("Duplicate API key entry '{}'", key);
        }
    }

    Ok(keys)
}
