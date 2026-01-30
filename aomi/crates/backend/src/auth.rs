use crate::namespace::{Namespace, DEFAULT_NAMESPACE, DEFAULT_NAMESPACE_SET};

/// Authorized API key with its allowed namespaces.
#[derive(Clone, Debug)]
pub struct AuthorizedKey {
    pub key: String,
    pub label: Option<String>,
    pub is_active: bool,
    allowed_namespaces: Vec<String>,
}

impl AuthorizedKey {
    pub fn new(
        key: String,
        label: Option<String>,
        is_active: bool,
        namespaces: Vec<String>,
    ) -> Self {
        let allowed_namespaces = namespaces
            .into_iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        Self {
            key,
            label,
            is_active,
            allowed_namespaces,
        }
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
    pub requested: String,
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
            requested: requested_namespace
                .filter(|s| !s.is_empty())
                .unwrap_or(DEFAULT_NAMESPACE)
                .to_string(),
        }
    }

    /// Get the requested namespace as a parsed Namespace enum.
    pub fn requested_backend(&self) -> Option<Namespace> {
        Namespace::parse(&self.requested)
    }

    /// Check if the requested namespace is authorized.
    pub fn is_authorized(&self) -> bool {
        self.current_authorization
            .iter()
            .any(|ns| ns.eq_ignore_ascii_case(&self.requested))
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
                return;
            }
        }

        // Priority 3: Keep default (already set in constructor)
    }
}
