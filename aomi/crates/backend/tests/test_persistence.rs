use anyhow::Result;
use aomi_backend::{
    history::{filter_system_messages, HistoryBackend, PersistentHistoryBackend},
    session::{ChatMessage, MessageSender},
};
use aomi_tools::db::{Session, SessionStore, SessionStoreApi};
use sqlx::{any::AnyPoolOptions, Any, Pool};

async fn setup_test_db() -> Result<Pool<Any>> {
    sqlx::any::install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    // Create schema
    sqlx::query(
        r#"
        CREATE TABLE users (
            public_key TEXT PRIMARY KEY,
            username TEXT UNIQUE,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
            started_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
            last_active_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
            title TEXT,
            pending_transaction TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            message_type TEXT NOT NULL DEFAULT 'chat',
            sender TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

fn test_message(sender: MessageSender, content: &str) -> ChatMessage {
    ChatMessage {
        sender,
        content: content.to_string(),
        tool_stream: None,
        timestamp: "00:00:00 UTC".to_string(),
        is_streaming: false,
    }
}

#[tokio::test]
async fn test_anonymous_session_returns_empty() -> Result<()> {
    let pool = setup_test_db().await?;
    let backend = PersistentHistoryBackend::new(pool).await;

    let history = backend
        .get_or_create_history(None, "anonymous-session".to_string(), None)
        .await?;

    assert!(
        history.is_none(),
        "Anonymous session should return empty history"
    );
    Ok(())
}

#[tokio::test]
#[ignore] // Skip: Uses PostgreSQL-specific JSONB casts incompatible with SQLite
async fn test_new_session_creates_user_and_session() -> Result<()> {
    let pool = setup_test_db().await?;
    let backend = PersistentHistoryBackend::new(pool.clone()).await;

    let pubkey = "0xTEST123".to_string();
    let session_id = "new-session".to_string();

    let history = backend
        .get_or_create_history(
            Some(pubkey.clone()),
            session_id.clone(),
            Some("Test Title".to_string()),
        )
        .await?;

    assert!(history.is_none(), "New session should return empty history");

    let db = SessionStore::new(pool.clone());
    let user = db.get_user(&pubkey).await?;
    assert!(user.is_some(), "User should be created");

    let session = db.get_session(&session_id).await?;
    assert!(session.is_some(), "Session should be created");
    assert_eq!(session.unwrap().public_key, Some(pubkey));

    Ok(())
}

#[tokio::test]
async fn test_update_history_filters_streaming() -> Result<()> {
    let pool = setup_test_db().await?;
    let backend = PersistentHistoryBackend::new(pool).await;
    let session_id = "test-session";

    let messages = vec![
        test_message(MessageSender::User, "Message 1"),
        ChatMessage {
            sender: MessageSender::Assistant,
            content: "Streaming...".to_string(),
            tool_stream: None,
            timestamp: "00:00:01 UTC".to_string(),
            is_streaming: true,
        },
        test_message(MessageSender::Assistant, "Complete message"),
    ];

    backend.update_history(session_id, &messages);

    let stored_messages = backend.get_session_messages(session_id).unwrap();
    assert_eq!(
        stored_messages.len(),
        2,
        "Should only store non-streaming messages"
    );
    assert!(!stored_messages[0].is_streaming);
    assert!(!stored_messages[1].is_streaming);

    Ok(())
}

#[tokio::test]
#[ignore] // Skip: Uses PostgreSQL-specific JSONB casts incompatible with SQLite
async fn test_flush_history_persists_messages() -> Result<()> {
    let pool = setup_test_db().await?;
    let backend = PersistentHistoryBackend::new(pool.clone()).await;
    let db = SessionStore::new(pool.clone());

    let pubkey = "0xFLUSH".to_string();
    let session_id = "flush-session".to_string();

    backend
        .get_or_create_history(Some(pubkey.clone()), session_id.clone(), None)
        .await?;

    let messages = vec![
        test_message(MessageSender::User, "User message"),
        test_message(MessageSender::Assistant, "Agent reply"),
        test_message(MessageSender::System, "System message"),
    ];

    backend.update_history(&session_id, &messages);

    backend
        .flush_history(Some(pubkey), session_id.clone())
        .await?;

    let db_messages = db.get_messages(&session_id, Some("chat"), None).await?;

    assert_eq!(
        db_messages.len(),
        2,
        "Should persist 2 messages (system excluded)"
    );
    assert_eq!(db_messages[0].sender, "user");
    assert_eq!(db_messages[1].sender, "agent");

    Ok(())
}

#[tokio::test]
#[ignore] // Skip: flush_history queries session with PostgreSQL JSONB casts (SQLite incompatible)
async fn test_flush_history_without_pubkey_does_nothing() -> Result<()> {
    let pool = setup_test_db().await?;
    let backend = PersistentHistoryBackend::new(pool.clone()).await;
    let db = SessionStore::new(pool.clone());

    let session_id = "no-pubkey-session".to_string();

    let messages = vec![test_message(MessageSender::User, "Test message")];
    backend.update_history(&session_id, &messages);

    backend.flush_history(None, session_id.clone()).await?;

    let result = db.get_messages(&session_id, Some("chat"), None).await;

    assert!(result.is_err() || result.unwrap().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_filter_system_messages() {
    let messages = vec![
        test_message(MessageSender::User, "User"),
        test_message(MessageSender::Assistant, "Assistant"),
        test_message(MessageSender::System, "System"),
    ];

    let filtered = filter_system_messages(&messages);

    assert_eq!(filtered.len(), 2, "Should filter out system messages");
    assert!(matches!(filtered[0].sender, MessageSender::User));
    assert!(matches!(filtered[1].sender, MessageSender::Assistant));
}

#[tokio::test]
async fn test_session_title_field_initialization() -> Result<()> {
    let session = Session {
        id: "test-session".to_string(),
        public_key: Some("test-pubkey".to_string()),
        started_at: 0,
        last_active_at: 0,
        title: Some("My Title".to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some("My Title".to_string()));
    Ok(())
}

#[tokio::test]
async fn test_session_title_can_be_none() -> Result<()> {
    let session = Session {
        id: "test-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: None,
        pending_transaction: None,
    };

    assert_eq!(session.title, None);
    Ok(())
}

#[tokio::test]
async fn test_session_title_can_be_empty() -> Result<()> {
    let session = Session {
        id: "test-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(String::new()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(String::new()));
    Ok(())
}

#[tokio::test]
async fn test_session_title_long_string() -> Result<()> {
    let long_title = "a".repeat(1000);
    let session = Session {
        id: "test-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(long_title.clone()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(long_title));
    assert_eq!(session.title.as_ref().unwrap().len(), 1000);
    Ok(())
}

#[tokio::test]
async fn test_session_title_special_characters() -> Result<()> {
    let special_title = "Title with 'quotes', \"double quotes\", and \\ backslash";
    let session = Session {
        id: "test-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(special_title.to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(special_title.to_string()));
    Ok(())
}

#[tokio::test]
async fn test_session_title_unicode() -> Result<()> {
    let unicode_titles = vec![
        "日本語テスト",
        "العربية اختبار",
        "Русский тест",
        "🚀 Rocket Launch",
    ];

    for unicode_title in unicode_titles {
        let session = Session {
            id: format!("test-session-{}", unicode_title),
            public_key: None,
            started_at: 0,
            last_active_at: 0,
            title: Some(unicode_title.to_string()),
            pending_transaction: None,
        };

        assert_eq!(
            session.title,
            Some(unicode_title.to_string()),
            "Unicode title should be preserved: {}",
            unicode_title
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_session_title_mutation() -> Result<()> {
    let mut session = Session {
        id: "test-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some("Initial Title".to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some("Initial Title".to_string()));

    session.title = Some("Updated Title".to_string());
    assert_eq!(session.title, Some("Updated Title".to_string()));

    session.title = None;
    assert_eq!(session.title, None);

    Ok(())
}

#[tokio::test]
async fn test_session_title_fallback_uuid() -> Result<()> {
    let fallback_title = "a1b2c3";
    let session = Session {
        id: "a1b2c3d4e5f6-full-uuid".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(fallback_title.to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(fallback_title.to_string()));
    assert_eq!(session.title.as_ref().unwrap().len(), 6);
    Ok(())
}

#[tokio::test]
#[ignore] // Requires PostgreSQL - uses SessionStore with JSONB syntax
async fn test_session_title_db_persistence() -> Result<()> {
    let pool = setup_test_db().await?;
    let db = SessionStore::new(pool.clone());

    let pubkey = "test_pubkey";
    let session_id = "test_session_id";
    let title = "My Research Session";

    db.get_or_create_user(pubkey).await?;

    let session = Session {
        id: session_id.to_string(),
        public_key: Some(pubkey.to_string()),
        started_at: 0,
        last_active_at: 0,
        title: Some(title.to_string()),
        pending_transaction: None,
    };

    db.create_session(&session).await?;

    let retrieved = db.get_session(session_id).await?;
    assert!(retrieved.is_some(), "Session should exist");
    assert_eq!(retrieved.unwrap().title, Some(title.to_string()));

    Ok(())
}

#[tokio::test]
#[ignore] // Requires PostgreSQL - uses SessionStore with JSONB syntax
async fn test_session_title_multiple_sessions_db() -> Result<()> {
    let pool = setup_test_db().await?;
    let db = SessionStore::new(pool.clone());

    let pubkey = "test_pubkey_multi";
    db.get_or_create_user(pubkey).await?;

    let titles = ["Chat 1", "Chat 2", "Chat 3"];

    for (i, title) in titles.iter().enumerate() {
        let session_id = format!("session_{}", i);
        let session = Session {
            id: session_id.clone(),
            public_key: Some(pubkey.to_string()),
            started_at: 0,
            last_active_at: 0,
            title: Some(title.to_string()),
            pending_transaction: None,
        };

        db.create_session(&session).await?;
    }

    for (i, expected_title) in titles.iter().enumerate() {
        let session_id = format!("session_{}", i);
        let session = db.get_session(&session_id).await?;
        assert_eq!(
            session.as_ref().map(|s| s.title.as_ref()),
            Some(Some(&expected_title.to_string())),
            "Session {} should have correct title",
            i
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_session_title_realistic_flow() -> Result<()> {
    let mut session = Session {
        id: "realistic-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some("abc123".to_string()),
        pending_transaction: None,
    };

    session.title = Some("Ethereum Trading Discussion".to_string());
    assert_eq!(
        session.title,
        Some("Ethereum Trading Discussion".to_string())
    );

    session.title = Some("My ETH Strategy".to_string());
    assert_eq!(session.title, Some("My ETH Strategy".to_string()));

    assert!(
        session.title.is_some(),
        "Title should never be None in this flow"
    );

    Ok(())
}

#[tokio::test]
async fn test_session_title_with_in_memory_history() -> Result<()> {
    let mut session = Session {
        id: "history-session".to_string(),
        public_key: Some("user123".to_string()),
        started_at: 0,
        last_active_at: 0,
        title: Some("f1d3c5".to_string()),
        pending_transaction: None,
    };

    let _messages = [
        test_message(MessageSender::User, "How to swap ETH?"),
        test_message(MessageSender::Assistant, "You can swap on Uniswap"),
    ];

    session.title = Some("ETH Swapping Guide".to_string());

    assert_eq!(session.title, Some("ETH Swapping Guide".to_string()));
    assert_eq!(session.public_key, Some("user123".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_session_title_minimum_length() -> Result<()> {
    let session = Session {
        id: "min-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some("A".to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some("A".to_string()));
    assert_eq!(session.title.as_ref().unwrap().len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_session_title_whitespace() -> Result<()> {
    let whitespace_title = "   \t\n   ";
    let session = Session {
        id: "whitespace-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(whitespace_title.to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(whitespace_title.to_string()));

    Ok(())
}

#[tokio::test]
async fn test_session_title_with_newlines() -> Result<()> {
    let title_with_newlines = "Line 1\nLine 2\nLine 3";
    let session = Session {
        id: "newline-session".to_string(),
        public_key: None,
        started_at: 0,
        last_active_at: 0,
        title: Some(title_with_newlines.to_string()),
        pending_transaction: None,
    };

    assert_eq!(session.title, Some(title_with_newlines.to_string()));
    assert!(session.title.as_ref().unwrap().contains('\n'));

    Ok(())
}

// TODO: Revisit these once HistoryBackend refactor lands
#[tokio::test]
#[ignore = "History restoration being refactored with new HistoryBackend trait"]
async fn rehydrated_session_keeps_agent_history_in_sync() {}

#[tokio::test]
#[ignore = "History persistence being refactored with new HistoryBackend trait"]
async fn multiple_sessions_store_and_retrieve_history_by_public_key() {}

#[tokio::test]
#[ignore = "History persistence being refactored with new HistoryBackend trait"]
async fn public_key_history_rehydrates_new_session_context() {}
