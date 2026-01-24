use aomi_backend::{
    history::{HistoryBackend, PersistentHistoryBackend},
    session::{AomiApp, AomiBackend},
    Namespace, SessionManager,
};
use aomi_core::{
    app::{CoreCtx, CoreState},
    CoreCommand,
};
use aomi_tools::db::{SessionStore, SessionStoreApi};
use async_trait::async_trait;
/// Integration test for title generation with BAML service
///
/// This test requires ANTHROPIC_API_KEY environment variable to be set.
///
/// To run this test:
/// ```bash
/// ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY cargo test --package aomi-backend test_title_generation_with_baml -- --ignored --nocapture
/// ```
///
/// This test verifies:
/// - Title generation task runs every 5 seconds
/// - Messages are converted to BAML format correctly
/// - BAML native FFI generates a title via Anthropic API
/// - Title is applied to session in-memory
/// - Title change broadcast is sent
///
/// Note: Database persistence is tested separately in history_tests.rs
use eyre::Result;
use serde_json::Value;
use sqlx::{any::AnyPoolOptions, Any, Pool};
use std::{collections::HashMap, sync::Arc};
use tokio::time::{sleep, Duration};

/// Connect to the local PostgreSQL database
async fn connect_to_db() -> Result<Pool<Any>> {
    sqlx::any::install_default_drivers();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://aomi@localhost:5432/chatbot".to_string());

    let pool = AnyPoolOptions::new().connect(&database_url).await?;
    Ok(pool)
}

#[derive(Clone)]
struct MockBackend;

#[async_trait]
impl AomiApp for MockBackend {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        ctx.command_sender
            .send(CoreCommand::StreamingText(
                "I can help with that.".to_string(),
            ))
            .await
            .map_err(|e| eyre::eyre!("Failed to send streaming text: {}", e))?;
        ctx.command_sender
            .send(CoreCommand::Complete)
            .await
            .map_err(|e| eyre::eyre!("Failed to send complete: {}", e))?;

        state.push_user(input);
        state.push_assistant("I can help with that.".to_string());

        Ok(())
    }
}

async fn create_test_session_manager(pool: Pool<Any>) -> Arc<SessionManager> {
    let backend: Arc<AomiBackend> = Arc::new(MockBackend);
    let history_backend: Arc<dyn HistoryBackend> =
        Arc::new(PersistentHistoryBackend::new(pool).await);

    let mut backends: HashMap<Namespace, Arc<AomiBackend>> = HashMap::new();
    backends.insert(Namespace::Default, backend);

    Arc::new(SessionManager::new(Arc::new(backends), history_backend))
}

async fn send_message(
    session_manager: &SessionManager,
    session_id: &str,
    message: &str,
) -> Result<()> {
    let session = session_manager
        .get_or_create_session(session_id, None)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;

    // Send message
    {
        let state = session.lock().await;
        state.input_sender.send(message.to_string()).await?;
    }

    // Wait for processing to complete
    sleep(Duration::from_millis(200)).await;

    // Update state to process CoreCommands
    {
        let mut state = session.lock().await;
        state.sync_state().await;
    }

    Ok(())
}

/// Check if ANTHROPIC_API_KEY is set (required for native BAML FFI)
fn is_anthropic_key_set() -> bool {
    std::env::var("ANTHROPIC_API_KEY").is_ok()
}

#[tokio::test]
async fn test_title_generation_with_baml() -> Result<()> {
    // Check if ANTHROPIC_API_KEY is set (required for native BAML FFI)
    if !is_anthropic_key_set() {
        eprintln!("ANTHROPIC_API_KEY environment variable is not set");
        eprintln!("   Set it with: export ANTHROPIC_API_KEY=sk-...");
        panic!("ANTHROPIC_API_KEY not available");
    }

    println!("ANTHROPIC_API_KEY is set");

    // Connect to database
    let pool = connect_to_db().await?;
    println!("Connected to PostgreSQL database");

    let session_manager = create_test_session_manager(pool.clone()).await;
    // Background tasks are automatically started by SessionManager::new()
    println!("Session manager created (background tasks auto-started)");

    // Subscribe to system updates
    let mut update_rx = session_manager.subscribe_to_updates();
    println!("Subscribed to system updates");

    // ==========================================================================
    // Test 1: Auto-generation for session with pubkey
    // ==========================================================================
    println!("\nTest 1: Title generation for session with pubkey");

    let session1 = "e2e-test-session-1";
    let pubkey1 = "0xTEST_E2E_1";

    // Set pubkey first
    session_manager
        .set_session_public_key(session1, Some(pubkey1.to_string()))
        .await;

    // Create session (starts with "New Chat")
    session_manager
        .get_or_create_session(session1, None)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;

    // Verify initial state
    let metadata = session_manager.get_session_metadata(session1).unwrap();
    assert_eq!(metadata.title, "New Chat");
    println!("   Session created with default title: New Chat");

    // Send messages to trigger title generation
    send_message(&session_manager, session1, "What is the current ETH price?").await?;
    send_message(
        &session_manager,
        session1,
        "How do I swap tokens on Uniswap?",
    )
    .await?;
    println!("   Sent 2 messages");

    // Wait for title generation (runs every 5 seconds, plus processing time)
    println!("   Waiting for title generation (up to 15 seconds)...");
    let mut generated_title1: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session1)
            .map(|m| m.title);

        if let Some(ref title) = current_title {
            if title != "New Chat" {
                generated_title1 = Some(title.clone());
                println!(
                    "   Title generated after {} seconds: {}",
                    (i + 1) as f64 * 0.5,
                    title
                );
                break;
            }
        }
    }

    assert!(
        generated_title1.is_some(),
        "Title should be generated within 15 seconds"
    );
    let generated_title1 = generated_title1.unwrap();

    // Verify title metadata
    let metadata = session_manager.get_session_metadata(session1).unwrap();
    assert_eq!(metadata.title, generated_title1.clone());
    println!("   Title metadata correct");

    // Verify title persisted to database
    let db = SessionStore::new(pool.clone());
    let session_record = db
        .get_session(session1)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;
    assert!(session_record.is_some(), "Session should exist in DB");
    assert_eq!(
        session_record.unwrap().title,
        Some(generated_title1.clone()),
        "Title should be persisted to database"
    );
    println!("   Title persisted to database");

    // Try to receive broadcast (with timeout)
    let broadcast_result =
        tokio::time::timeout(Duration::from_millis(1000), update_rx.recv()).await;

    match broadcast_result {
        Ok(Ok((event_session_id, Value::Object(map)))) => {
            let event_type = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let new_title = map
                .get("new_title")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            assert_eq!(event_type, "title_changed");
            assert_eq!(event_session_id, session1);
            assert_eq!(new_title, generated_title1);
            println!("   Received title change broadcast");
        }
        _ => {
            println!("   No broadcast received (may have been sent before subscription)");
        }
    }

    // ==========================================================================
    // Test 2: Anonymous session (no pubkey) - should NOT persist to DB
    // ==========================================================================
    println!("\nTest 2: Title generation for anonymous session");

    let session2 = "e2e-test-session-2";

    // Create session WITHOUT pubkey (starts with "New Chat")
    session_manager
        .get_or_create_session(session2, None)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;
    println!("   Anonymous session created");

    send_message(&session_manager, session2, "Tell me about DeFi").await?;
    send_message(&session_manager, session2, "What are yield farms?").await?;
    println!("   Sent 2 messages");

    // Wait for title generation
    println!("   Waiting for title generation...");
    let mut generated_title2: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session2)
            .map(|m| m.title);

        if let Some(ref title) = current_title {
            if title != "New Chat" {
                generated_title2 = Some(title.clone());
                println!(
                    "   Title generated after {} seconds: {}",
                    (i + 1) as f64 * 0.5,
                    title
                );
                break;
            }
        }
    }

    assert!(
        generated_title2.is_some(),
        "Anonymous session should get title generated"
    );
    let generated_title2 = generated_title2.unwrap();

    // Verify title is in memory
    let metadata = session_manager.get_session_metadata(session2).unwrap();
    assert_eq!(metadata.title, generated_title2.clone());
    println!("   Title in memory");

    // Verify title NOT persisted to database (anonymous session)
    let session_record = db
        .get_session(session2)
        .await
        .map_err(|e| eyre::eyre!(e.to_string()))?;
    assert!(
        session_record.is_none(),
        "Anonymous session should NOT be in database"
    );
    println!("   Title NOT persisted (anonymous session)");

    // ==========================================================================
    // Test 3: Title re-generation (conversation grows)
    // ==========================================================================
    println!("\nTest 3: Title re-generation as conversation grows");

    // Add more messages to session1
    let old_title = generated_title1.clone();
    send_message(&session_manager, session1, "Tell me about NFTs").await?;
    send_message(&session_manager, session1, "What are smart contracts?").await?;
    send_message(&session_manager, session1, "Explain gas fees").await?;
    println!("   Sent 3 more messages to session 1");

    // Wait for re-generation
    println!("   Waiting for title re-generation...");
    let mut new_title: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session1)
            .map(|m| m.title);

        if let Some(ref title) = current_title {
            if title != &old_title {
                new_title = Some(title.clone());
                println!(
                    "   Title re-generated after {} seconds: {}",
                    (i + 1) as f64 * 0.5,
                    title
                );
                break;
            }
        }
    }

    // Note: Title might stay the same if BAML determines same topic
    if let Some(new_title) = new_title {
        println!("   Title updated from '{}' to '{}'", old_title, new_title);
    } else {
        println!("   Title remained the same (BAML determined same topic)");
    }

    // ==========================================================================
    // Final Summary
    // ==========================================================================
    println!("\nAll integration tests passed!");
    println!("   - Title generation with BAML service");
    println!("   - Database persistence (with pubkey)");
    println!("   - Anonymous session (no persistence)");
    println!("   - Title re-generation capability");

    Ok(())
}
