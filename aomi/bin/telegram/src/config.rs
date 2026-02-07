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
    #[serde(default)]
    pub allow_from: Vec<i64>,
}

impl TelegramConfig {
    pub fn from_path(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read bot config {}: {}", path, e))?;
        let config: TelegramConfig =
            toml::from_str(&contents).map_err(|e| anyhow::anyhow!("Invalid bot config: {}", e))?;
        Ok(config)
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN environment variable is required"))?;

        let dm_policy = std::env::var("TELEGRAM_DM_POLICY")
            .unwrap_or_else(|_| "open".to_string())
            .parse()
            .unwrap_or(DmPolicy::Open);

        let group_policy = std::env::var("TELEGRAM_GROUP_POLICY")
            .unwrap_or_else(|_| "mention".to_string())
            .parse()
            .unwrap_or(GroupPolicy::Mention);

        let allow_from: Vec<i64> = std::env::var("TELEGRAM_ALLOW_FROM")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        let backend_url = std::env::var("AOMI_BACKEND_URL").ok();

        Ok(Self {
            enabled: true,
            bot_token,
            dm_policy,
            group_policy,
            backend_url,
            allow_from,
        })
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
