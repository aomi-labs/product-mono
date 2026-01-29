//! Centralized namespace definitions and backend registration.
//!
//! This module provides a single source of truth for:
//! - The `Namespace` enum
//! - Backend factory functions
//! - Magic string parsing for backend selection

use std::{collections::HashMap, sync::Arc};

use crate::types::AomiBackend;
use anyhow::Result;
pub use aomi_core::BuildOpts;
use aomi_admin::AdminApp;
use aomi_core::CoreApp;
use aomi_forge::ForgeApp;
use aomi_l2beat::L2BeatApp;
use aomi_polymarket::PolymarketApp;

pub const DEFAULT_NAMESPACE: &str = "default";

/// Backend namespace variants
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    Default,
    L2b,
    Forge,
    Admin,
    Polymarket,
    Test,
}

impl Namespace {
    /// Parse namespace from string (case-insensitive)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default" => Some(Namespace::Default),
            "l2beat" | "l2b" => Some(Namespace::L2b),
            "forge" => Some(Namespace::Forge),
            "admin" => Some(Namespace::Admin),
            "polymarket" => Some(Namespace::Polymarket),
            "test" => Some(Namespace::Test),
            _ => None,
        }
    }

    /// Check if this is the default namespace
    pub fn is_default(&self) -> bool {
        matches!(self, Namespace::Default)
    }
}

/// Type alias for backend registry map
pub type BackendMappings = HashMap<Namespace, Arc<AomiBackend>>;

/// Build backends from configurations
pub async fn build_backends(configs: Vec<(Namespace, BuildOpts)>) -> Result<BackendMappings> {
    let mut backends = HashMap::new();

    for (namespace, opts) in configs {
        let backend: Arc<AomiBackend> = match namespace {
            Namespace::Polymarket => {
                let app = Arc::new(
                    PolymarketApp::default()
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Default => {
                let app = Arc::new(
                    CoreApp::new(opts)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::L2b => {
                let app = Arc::new(
                    L2BeatApp::new(opts)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Forge => {
                let app = Arc::new(
                    ForgeApp::new(opts)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Admin => {
                let app = Arc::new(
                    AdminApp::new(opts)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
            Namespace::Test => {
                let app = Arc::new(
                    CoreApp::new(opts)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                );
                app
            }
        };

        backends.insert(namespace, backend);
    }

    Ok(backends)
}

/// Parse a message for magic strings that indicate backend selection.
///
/// Magic strings:
/// - `default-magic` -> Namespace::Default
/// - `l2beat-magic` -> Namespace::L2b
/// - `forge-magic` -> Namespace::Forge
/// - `admin-magic` -> Namespace::Admin
/// - `polymarket-magic` -> Namespace::Polymarket
/// - `test-magic` -> Namespace::Test
pub fn get_backend_request(message: &str) -> Option<Namespace> {
    let normalized = message.to_lowercase();

    match normalized.as_str() {
        s if s.contains("default-magic") => Some(Namespace::Default),
        s if s.contains("l2beat-magic") => Some(Namespace::L2b),
        s if s.contains("forge-magic") => Some(Namespace::Forge),
        s if s.contains("admin-magic") => Some(Namespace::Admin),
        s if s.contains("polymarket-magic") => Some(Namespace::Polymarket),
        s if s.contains("test-magic") => Some(Namespace::Test),
        _ => None,
    }
}

/// Check if namespace string is NOT the default namespace (case-insensitive)
pub fn is_not_default(namespace: &str) -> bool {
    !namespace.eq_ignore_ascii_case(DEFAULT_NAMESPACE)
}

/// Extract and trim namespace from optional string
pub fn extract_namespace(s: Option<&String>) -> &str {
    s.map(String::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_NAMESPACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_backend_request() {
        assert_eq!(
            get_backend_request("use default-magic"),
            Some(Namespace::Default)
        );
        assert_eq!(
            get_backend_request("l2beat-magic please"),
            Some(Namespace::L2b)
        );
        assert_eq!(get_backend_request("FORGE-MAGIC"), Some(Namespace::Forge));
        assert_eq!(
            get_backend_request("polymarket-magic bet"),
            Some(Namespace::Polymarket)
        );
        assert_eq!(
            get_backend_request("test-magic here"),
            Some(Namespace::Test)
        );
        assert_eq!(get_backend_request("no magic here"), None);
    }

    #[test]
    fn test_namespace_parse() {
        assert_eq!(Namespace::parse("default"), Some(Namespace::Default));
        assert_eq!(Namespace::parse("DEFAULT"), Some(Namespace::Default));
        assert_eq!(Namespace::parse("l2beat"), Some(Namespace::L2b));
        assert_eq!(Namespace::parse("forge"), Some(Namespace::Forge));
        assert_eq!(Namespace::parse("unknown"), None);
    }

    #[test]
    fn test_is_not_default() {
        assert!(!is_not_default("default"));
        assert!(!is_not_default("DEFAULT"));
        assert!(is_not_default("l2beat"));
        assert!(is_not_default("forge"));
    }

    #[test]
    fn test_extract_namespace() {
        assert_eq!(extract_namespace(Some(&"l2beat".to_string())), "l2beat");
        assert_eq!(extract_namespace(Some(&"  forge  ".to_string())), "forge");
        assert_eq!(extract_namespace(Some(&"".to_string())), "default");
        assert_eq!(extract_namespace(None), "default");
    }
}
