//! Policy enforcement for chat bots.

use std::collections::HashSet;

/// Policy for handling direct messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmPolicy {
    /// Process all DMs from anyone
    #[default]
    Open,
    /// Only process DMs from allowlisted users
    Allowlist,
    /// Ignore all DMs
    Disabled,
}

/// Policy for handling group messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupPolicy {
    /// Only respond when bot is mentioned
    #[default]
    Mention,
    /// Respond to all messages in the group
    Always,
    /// Ignore all group messages
    Disabled,
}

/// Configuration for bot policies.
#[derive(Debug, Clone, Default)]
pub struct PolicyConfig {
    /// Policy for direct messages
    pub dm_policy: DmPolicy,
    /// Policy for group messages  
    pub group_policy: GroupPolicy,
    /// Set of allowlisted user IDs (used when dm_policy is Allowlist)
    pub allowlist: HashSet<String>,
    /// Set of blocked user IDs (always ignored)
    pub blocklist: HashSet<String>,
}

impl PolicyConfig {
    /// Create a new policy config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the DM policy.
    pub fn with_dm_policy(mut self, policy: DmPolicy) -> Self {
        self.dm_policy = policy;
        self
    }

    /// Set the group policy.
    pub fn with_group_policy(mut self, policy: GroupPolicy) -> Self {
        self.group_policy = policy;
        self
    }

    /// Add a user to the allowlist.
    pub fn allow_user(mut self, user_id: impl Into<String>) -> Self {
        self.allowlist.insert(user_id.into());
        self
    }

    /// Add multiple users to the allowlist.
    pub fn allow_users(mut self, user_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for id in user_ids {
            self.allowlist.insert(id.into());
        }
        self
    }

    /// Add a user to the blocklist.
    pub fn block_user(mut self, user_id: impl Into<String>) -> Self {
        self.blocklist.insert(user_id.into());
        self
    }

    /// Check if a user is allowlisted.
    pub fn is_allowlisted(&self, user_id: &str) -> bool {
        self.allowlist.contains(user_id)
    }

    /// Check if a user is blocklisted.
    pub fn is_blocked(&self, user_id: &str) -> bool {
        self.blocklist.contains(user_id)
    }

    /// Check if a DM from this user should be processed.
    pub fn should_process_dm(&self, user_id: &str) -> bool {
        if self.is_blocked(user_id) {
            return false;
        }

        match self.dm_policy {
            DmPolicy::Open => true,
            DmPolicy::Allowlist => self.is_allowlisted(user_id),
            DmPolicy::Disabled => false,
        }
    }

    /// Check if a group message should be processed.
    pub fn should_process_group(&self, user_id: &str, is_mention: bool) -> bool {
        if self.is_blocked(user_id) {
            return false;
        }

        match self.group_policy {
            GroupPolicy::Always => true,
            GroupPolicy::Mention => is_mention,
            GroupPolicy::Disabled => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dm_policy_open() {
        let config = PolicyConfig::new().with_dm_policy(DmPolicy::Open);
        assert!(config.should_process_dm("anyone"));
    }

    #[test]
    fn test_dm_policy_allowlist() {
        let config = PolicyConfig::new()
            .with_dm_policy(DmPolicy::Allowlist)
            .allow_user("123");

        assert!(config.should_process_dm("123"));
        assert!(!config.should_process_dm("456"));
    }

    #[test]
    fn test_dm_policy_disabled() {
        let config = PolicyConfig::new().with_dm_policy(DmPolicy::Disabled);
        assert!(!config.should_process_dm("anyone"));
    }

    #[test]
    fn test_blocklist_overrides() {
        let config = PolicyConfig::new()
            .with_dm_policy(DmPolicy::Open)
            .block_user("baduser");

        assert!(config.should_process_dm("gooduser"));
        assert!(!config.should_process_dm("baduser"));
    }

    #[test]
    fn test_group_policy_mention() {
        let config = PolicyConfig::new().with_group_policy(GroupPolicy::Mention);
        assert!(config.should_process_group("user", true));
        assert!(!config.should_process_group("user", false));
    }

    #[test]
    fn test_group_policy_always() {
        let config = PolicyConfig::new().with_group_policy(GroupPolicy::Always);
        assert!(config.should_process_group("user", false));
    }
}
