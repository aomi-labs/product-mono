//! Centralized namespace definitions and backend registration.
//!
//! This module provides a single source of truth for:
//! - The `Namespace` enum (re-exported from aomi_backend)
//! - Backend map type alias
//! - Magic string parsing for backend selection

use std::{collections::HashMap, sync::Arc};

// Re-export Namespace from the crate
pub use aomi_backend::Namespace;
use aomi_backend::session::AomiBackend;

/// Type alias for the backend registry map.
#[allow(dead_code)]
pub type BackendMap = HashMap<Namespace, Arc<AomiBackend>>;

/// Parse a message for magic strings that indicate backend selection.
///
/// Magic strings:
/// - `default-magic` -> Namespace::Default
/// - `l2beat-magic` -> Namespace::L2b
/// - `forge-magic` -> Namespace::Forge
/// - `polymarket-magic` -> Namespace::Polymarket
/// - `test-magic` -> Namespace::Test
pub fn get_backend_request(message: &str) -> Option<Namespace> {
    let normalized = message.to_lowercase();

    match normalized.as_str() {
        s if s.contains("default-magic") => Some(Namespace::Default),
        s if s.contains("l2beat-magic") => Some(Namespace::L2b),
        s if s.contains("forge-magic") => Some(Namespace::Forge),
        s if s.contains("polymarket-magic") => Some(Namespace::Polymarket),
        s if s.contains("test-magic") => Some(Namespace::Test),
        _ => None,
    }
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
        assert_eq!(
            get_backend_request("FORGE-MAGIC"),
            Some(Namespace::Forge)
        );
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
}
