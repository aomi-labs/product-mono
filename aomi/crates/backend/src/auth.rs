use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::{AnyPool, Row};

use crate::namespace::{Namespace, DEFAULT_NAMESPACE, DEFAULT_NAMESPACE_SET};

/// Authorized API key with its allowed namespaces.
#[derive(Clone, Debug)]
pub struct AuthorizedKey {
    pub key: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub pool: Arc<AnyPool>,
    allowed_namespaces: Vec<String>,
}

impl AuthorizedKey {
    /// Load an API key from the database. Returns `None` if the key doesn't
    /// exist or is inactive.
    pub async fn new(pool: Arc<AnyPool>, key: &str) -> Result<Option<Self>> {
        let rows = sqlx::query(
            "SELECT api_key, label, namespace, \
             CAST(is_active AS INTEGER) AS is_active \
             FROM api_keys WHERE api_key = $1 AND is_active = TRUE",
        )
        .bind(key)
        .fetch_all(pool.as_ref())
        .await
        .context("Failed to query api_keys table")?;

        let Some(first_row) = rows.first() else {
            return Ok(None);
        };

        let api_key: String = first_row
            .try_get("api_key")
            .context("Failed to read api_key")?;
        let label: Option<String> = first_row.try_get("label").context("Failed to read label")?;
        let is_active: i32 = first_row
            .try_get("is_active")
            .context("Failed to read is_active")?;

        let allowed_namespaces: Vec<String> = rows
            .into_iter()
            .filter_map(|row| row.try_get::<String, _>("namespace").ok())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Some(Self {
            key: api_key,
            label,
            is_active: is_active != 0,
            pool,
            allowed_namespaces,
        }))
    }

    pub fn get_allowed_namespaces(&self) -> &[String] {
        &self.allowed_namespaces
    }

    /// Check if a specific namespace is allowed for this API key.
    pub fn allows_namespace(&self, namespace: &str) -> bool {
        self.allowed_namespaces
            .iter()
            .any(|ns| ns.eq_ignore_ascii_case(namespace))
    }
}

/// Namespace authorization context for a request.
///
/// This struct captures all the authorization information needed to validate
/// and process a namespace request.
#[derive(Clone, Debug)]
pub struct NamespaceAuth {
    /// User's public key (wallet address)
    pub pub_key: Option<String>,
    /// API key authorization (if provided)
    pub api_key: Option<AuthorizedKey>,
    /// Current merged authorization (union of API key + user namespaces)
    pub current_authorization: Vec<String>,
    /// The requested namespace (always has a value, defaults to "default")
    pub requested_namespace: String,
}

impl NamespaceAuth {
    /// Create a new NamespaceAuth with default authorization.
    ///
    /// - `current_authorization` is initialized to DEFAULT_NAMESPACE_SET
    /// - `requested` defaults to DEFAULT_NAMESPACE if not specified
    pub fn new(
        pub_key: Option<String>,
        api_key: Option<AuthorizedKey>,
        requested_namespace: Option<&str>,
    ) -> Self {
        Self {
            pub_key,
            api_key,
            current_authorization: DEFAULT_NAMESPACE_SET
                .iter()
                .map(|s| s.to_string())
                .collect(),
            requested_namespace: requested_namespace
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_NAMESPACE)
                .to_string(),
        }
    }

    /// Get the requested namespace as a parsed Namespace enum.
    pub fn requested_backend(&self) -> Option<Namespace> {
        Namespace::parse(&self.requested_namespace)
    }

    /// Check if the requested namespace is authorized.
    pub fn is_authorized(&self) -> bool {
        self.current_authorization
            .iter()
            .any(|ns| ns.eq_ignore_ascii_case(&self.requested_namespace))
    }

    /// Fetch user namespaces from the database and merge into current_authorization.
    ///
    /// This is the primary entry point for resolving authorization. Call once after
    /// construction to populate the full authorization set.
    pub async fn resolve(&mut self, session_manager: &crate::manager::SessionManager) {
        let user_namespaces = if let Some(ref pk) = self.pub_key {
            session_manager.get_user_namespaces(pk).await.ok()
        } else {
            None
        };
        self.merge_authorization(user_namespaces);
    }

    /// Whether the requested namespace requires an API key (i.e. is not a default namespace).
    pub fn requires_api_key(&self) -> bool {
        !DEFAULT_NAMESPACE_SET
            .iter()
            .any(|ns| ns.eq_ignore_ascii_case(&self.requested_namespace))
    }

    /// Merge namespaces from API key and user into current_authorization.
    ///
    /// The merge strategy is:
    /// - If API key has namespaces, use those
    /// - Otherwise, if user has namespaces, use those
    /// - Otherwise, keep the default namespaces
    pub fn merge_authorization(&mut self, user_namespaces: Option<Vec<String>>) {
        // Priority 1: API key namespaces
        if let Some(ref api_key) = self.api_key {
            let api_namespaces = api_key.get_allowed_namespaces();
            if !api_namespaces.is_empty() {
                self.current_authorization = api_namespaces.to_vec();
                return;
            }
        }

        // Priority 2: User namespaces from database
        if let Some(namespaces) = user_namespaces {
            if !namespaces.is_empty() {
                self.current_authorization = namespaces;
            }
        }

        // Priority 3: Keep default (already set in constructor)
    }
}
