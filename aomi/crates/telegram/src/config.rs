use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DmPolicy {
    Open,
    Allowlist,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GroupPolicy {
    Mention,
    Always,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub dm_policy: DmPolicy,
    pub group_policy: GroupPolicy,
    #[serde(default)]
    pub allow_from: Vec<i64>,
}

impl TelegramConfig {
    pub fn is_allowlisted(&self, user_id: i64) -> bool {
        self.allow_from.iter().any(|id| *id == user_id)
    }
}
