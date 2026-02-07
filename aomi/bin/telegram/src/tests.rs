use serde_json::json;
use teloxide::types::{ChatId, UserId};

use crate::{
    config::{DmPolicy, GroupPolicy, TelegramConfig},
    session::{dm_session_key, group_session_key},
};

fn dm_policy_allows(config: &TelegramConfig, user_id: i64) -> bool {
    match config.dm_policy {
        DmPolicy::Disabled => false,
        DmPolicy::Allowlist => config.is_allowlisted(user_id),
        DmPolicy::Open => true,
    }
}

fn group_policy_allows(config: &TelegramConfig, is_mentioned: bool) -> bool {
    match config.group_policy {
        GroupPolicy::Disabled => false,
        GroupPolicy::Always => true,
        GroupPolicy::Mention => is_mentioned,
    }
}

#[test]
fn dm_session_key_format() {
    let key = dm_session_key(UserId(42));
    assert_eq!(key, "telegram:dm:42");
}

#[test]
fn group_session_key_format() {
    let key = group_session_key(ChatId(1234));
    assert_eq!(key, "telegram:group:1234");
}

#[test]
fn dm_session_keys_unique_per_user() {
    let key_one = dm_session_key(UserId(1));
    let key_two = dm_session_key(UserId(2));
    assert_ne!(key_one, key_two);
}

#[test]
fn group_session_keys_unique_per_chat() {
    let key_one = group_session_key(ChatId(10));
    let key_two = group_session_key(ChatId(20));
    assert_ne!(key_one, key_two);
}

#[test]
fn group_session_key_negative_ids() {
    let key = group_session_key(ChatId(-100_123));
    assert_eq!(key, "telegram:group:-100123");
}

#[test]
fn dm_policy_open_accepts_any_user() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Open,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(dm_policy_allows(&config, 999));
    assert!(dm_policy_allows(&config, -5));
}

#[test]
fn dm_policy_allowlist_accepts_only_listed_users() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Allowlist,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![101, 202],
    };

    assert!(dm_policy_allows(&config, 101));
    assert!(!dm_policy_allows(&config, 303));
}

#[test]
fn dm_policy_disabled_rejects_all() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Disabled,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![1],
    };

    assert!(!dm_policy_allows(&config, 1));
    assert!(!dm_policy_allows(&config, 999));
}

#[test]
fn group_policy_always_processes_messages() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Disabled,
        group_policy: GroupPolicy::Always,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(group_policy_allows(&config, false));
    assert!(group_policy_allows(&config, true));
}

#[test]
fn group_policy_mention_requires_mention() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Disabled,
        group_policy: GroupPolicy::Mention,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(!group_policy_allows(&config, false));
    assert!(group_policy_allows(&config, true));
}

#[test]
fn group_policy_disabled_rejects_all() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Disabled,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(!group_policy_allows(&config, false));
    assert!(!group_policy_allows(&config, true));
}

#[test]
fn is_allowlisted_true_for_listed_ids() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Allowlist,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![7, 8, 9],
    };

    assert!(config.is_allowlisted(8));
}

#[test]
fn is_allowlisted_false_for_unlisted_ids() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Allowlist,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![7, 8, 9],
    };

    assert!(!config.is_allowlisted(10));
}

#[test]
fn empty_allowlist_blocks_everyone() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Allowlist,
        group_policy: GroupPolicy::Disabled,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(!dm_policy_allows(&config, 1));
}

#[test]
fn config_serializes_to_json() {
    let config = TelegramConfig {
        enabled: true,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Allowlist,
        group_policy: GroupPolicy::Mention,
        backend_url: None,
        allow_from: vec![1, 2],
    };

    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(
        value,
        json!({
            "enabled": true,
            "bot_token": "token",
            "dm_policy": "allowlist",
            "group_policy": "mention",
            "backend_url": null,
            "allow_from": [1, 2]
        })
    );
}

#[test]
fn config_deserializes_from_json() {
    let value = json!({
        "enabled": false,
        "bot_token": "token",
        "dm_policy": "open",
        "group_policy": "always",
        "allow_from": [42]
    });

    let config: TelegramConfig = serde_json::from_value(value).unwrap();
    assert!(!config.enabled);
    assert_eq!(config.bot_token, "token");
    assert_eq!(config.dm_policy, DmPolicy::Open);
    assert_eq!(config.group_policy, GroupPolicy::Always);
    assert_eq!(config.allow_from, vec![42]);
    assert_eq!(config.backend_url, None);
}

#[test]
fn config_enabled_flag() {
    let config = TelegramConfig {
        enabled: false,
        bot_token: "token".to_string(),
        dm_policy: DmPolicy::Open,
        group_policy: GroupPolicy::Always,
        backend_url: None,
        allow_from: vec![],
    };

    assert!(!config.enabled);
}
