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
pub enum GuildPolicy {
    Mention,
    Always,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub dm_policy: DmPolicy,
    pub guild_policy: GuildPolicy,
    #[serde(default)]
    pub allow_from: Vec<u64>,
}

impl DiscordConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let bot_token = std::env::var("DISCORD_BOT_TOKEN")
            .map_err(|_| anyhow::anyhow!("DISCORD_BOT_TOKEN environment variable is required"))?;

        let dm_policy = std::env::var("DISCORD_DM_POLICY")
            .unwrap_or_else(|_| "open".to_string())
            .parse()
            .unwrap_or(DmPolicy::Open);

        let guild_policy = std::env::var("DISCORD_GUILD_POLICY")
            .unwrap_or_else(|_| "mention".to_string())
            .parse()
            .unwrap_or(GuildPolicy::Mention);

        let allow_from: Vec<u64> = std::env::var("DISCORD_ALLOW_FROM")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        Ok(Self {
            enabled: true,
            bot_token,
            dm_policy,
            guild_policy,
            allow_from,
        })
    }

    pub fn is_allowlisted(&self, user_id: u64) -> bool {
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

impl std::str::FromStr for GuildPolicy {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mention" => Ok(GuildPolicy::Mention),
            "always" => Ok(GuildPolicy::Always),
            "disabled" => Ok(GuildPolicy::Disabled),
            _ => Err(()),
        }
    }
}
