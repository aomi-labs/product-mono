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
    pub backend_url: Option<String>,
    pub mini_app_url: String,
    #[serde(default)]
    pub allow_from: Vec<i64>,
}

impl TelegramConfig {
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read bot config {}: {}", path, e))?;
        let mut config: TelegramConfig =
            toml::from_str(&contents).map_err(|e| anyhow::anyhow!("Invalid bot config: {}", e))?;
        let trimmed = config.mini_app_url.trim_end_matches('/').to_string();
        if !trimmed.starts_with("https://") {
            return Err(anyhow::anyhow!(
                "mini_app_url must be HTTPS for Telegram Web Apps: {}",
                config.mini_app_url
            ));
        }
        config.mini_app_url = trimmed;
        Ok(config)
    }

    pub fn is_allowlisted(&self, user_id: i64) -> bool {
        self.allow_from.contains(&user_id)
    }
}

impl std::str::FromStr for DmPolicy {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(DmPolicy::Open),
            "allowlist" => Ok(DmPolicy::Allowlist),
            "disabled" => Ok(DmPolicy::Disabled),
            _ => Err(()),
        }
    }
}

impl std::str::FromStr for GroupPolicy {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mention" => Ok(GroupPolicy::Mention),
            "always" => Ok(GroupPolicy::Always),
            "disabled" => Ok(GroupPolicy::Disabled),
            _ => Err(()),
        }
    }
}
