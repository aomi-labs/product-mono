//! Tests for Discord bot functionality.

use crate::config::{DiscordConfig, DmPolicy, GuildPolicy};
use crate::session::{dm_session_key, channel_session_key};
use serenity::model::id::{ChannelId, UserId};

#[test]
fn dm_session_key_format() {
    let key = dm_session_key(UserId::new(42));
    assert_eq!(key, "discord:dm:42");
}

#[test]
fn channel_session_key_format() {
    let key = channel_session_key(ChannelId::new(1234));
    assert_eq!(key, "discord:channel:1234");
}

#[test]
fn dm_session_keys_unique_per_user() {
    let key_one = dm_session_key(UserId::new(1));
    let key_two = dm_session_key(UserId::new(2));
    assert_ne!(key_one, key_two);
}

#[test]
fn channel_session_keys_unique_per_channel() {
    let key_one = channel_session_key(ChannelId::new(100));
    let key_two = channel_session_key(ChannelId::new(200));
    assert_ne!(key_one, key_two);
}

fn test_config() -> DiscordConfig {
    DiscordConfig {
        enabled: true,
        bot_token: "test-token".to_string(),
        dm_policy: DmPolicy::Open,
        guild_policy: GuildPolicy::Mention,
        allow_from: vec![123, 456],
    }
}

#[test]
fn dm_policy_open_allows_all() {
    let config = test_config();
    assert!(matches!(config.dm_policy, DmPolicy::Open));
}

#[test]
fn allowlist_check_works() {
    let config = test_config();
    assert!(config.is_allowlisted(123));
    assert!(config.is_allowlisted(456));
    assert!(!config.is_allowlisted(789));
}

#[test]
fn dm_policy_from_str() {
    assert_eq!("open".parse::<DmPolicy>(), Ok(DmPolicy::Open));
    assert_eq!("allowlist".parse::<DmPolicy>(), Ok(DmPolicy::Allowlist));
    assert_eq!("disabled".parse::<DmPolicy>(), Ok(DmPolicy::Disabled));
    assert_eq!("OPEN".parse::<DmPolicy>(), Ok(DmPolicy::Open));
    assert!("invalid".parse::<DmPolicy>().is_err());
}

#[test]
fn guild_policy_from_str() {
    assert_eq!("mention".parse::<GuildPolicy>(), Ok(GuildPolicy::Mention));
    assert_eq!("always".parse::<GuildPolicy>(), Ok(GuildPolicy::Always));
    assert_eq!("disabled".parse::<GuildPolicy>(), Ok(GuildPolicy::Disabled));
    assert_eq!("MENTION".parse::<GuildPolicy>(), Ok(GuildPolicy::Mention));
    assert!("invalid".parse::<GuildPolicy>().is_err());
}
