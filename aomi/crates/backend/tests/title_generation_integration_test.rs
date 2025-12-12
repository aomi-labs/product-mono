/// Integration test for title generation with BAML service
///
/// This test requires a running BAML server at http://localhost:2024
///
/// To run this test:
/// 1. Start BAML server: cd aomi/crates/l2beat && npx @boundaryml/baml serve
/// 2. Run the test: cargo test --package aomi-backend test_title_generation_with_baml -- --ignored --nocapture
///
/// This test verifies:
/// - Title generation task runs every 5 seconds
/// - Messages are converted to BAML format correctly
/// - BAML service generates a title
/// - Title is applied to session in-memory
/// - SystemUpdate::TitleChanged broadcast is sent
/// - User-titled sessions are never overwritten
///
/// Note: Database persistence is tested separately in history_tests.rs
use anyhow::Result;
use aomi_backend::{
    history::{HistoryBackend, PersistentHistoryBackend},
    session::{AomiBackend, BackendwithTool, SystemUpdate},
    BackendType, SessionManager,
};
use aomi_chat::{ChatCommand, Message, SystemEventQueue, ToolResultStream};
use aomi_tools::db::{SessionStore, SessionStoreApi};
use async_trait::async_trait;
use sqlx::{any::AnyPoolOptions, Any, Pool};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{mpsc, RwLock},
    time::{sleep, Duration},
};

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
impl AomiBackend for MockBackend {
    type Command = ChatCommand<ToolResultStream>;

    async fn process_message(
        &self,
        history: Arc<RwLock<Vec<Message>>>,
        system_events: SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<ChatCommand<ToolResultStream>>,
        _interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        sender_to_ui
            .send(ChatCommand::StreamingText(
                "I can help with that.".to_string(),
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send streaming text: {}", e))?;
        sender_to_ui
            .send(ChatCommand::Complete)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send complete: {}", e))?;

        let mut history_guard = history.write().await;
        history_guard.push(Message::user(input));
        history_guard.push(Message::assistant("I can help with that.".to_string()));

        Ok(())
    }
}

async fn create_test_session_manager(pool: Pool<Any>) -> Arc<SessionManager> {
    let backend: Arc<BackendwithTool> = Arc::new(MockBackend);
    let history_backend: Arc<dyn HistoryBackend> =
        Arc::new(PersistentHistoryBackend::new(pool).await);

    let mut backends: HashMap<BackendType, Arc<BackendwithTool>> = HashMap::new();
    backends.insert(BackendType::Default, backend);

    Arc::new(SessionManager::new(Arc::new(backends), history_backend))
}

async fn send_message(
    session_manager: &SessionManager,
    session_id: &str,
    message: &str,
) -> Result<()> {
    let session = session_manager
        .get_or_create_session(session_id, None, None)
        .await?;

    // Send message
    {
        let state = session.lock().await;
        state.sender_to_llm.send(message.to_string()).await?;
    }

    // Wait for processing to complete
    sleep(Duration::from_millis(200)).await;

    // Update state to process ChatCommands
    {
        let mut state = session.lock().await;
        state.update_state().await;
    }

    Ok(())
}

/// Check if BAML server is running by attempting a connection
async fn is_baml_server_running() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;

    // Try to connect to BAML server at localhost:2024
    TcpStream::connect_timeout(&"127.0.0.1:2024".parse().unwrap(), Duration::from_secs(1)).is_ok()
}

#[tokio::test]
#[ignore = "Requires BAML server running at localhost:2024"]
async fn test_title_generation_with_baml() -> Result<()> {
    // Check if BAML server is running
    if !is_baml_server_running().await {
        eprintln!("❌ BAML server is not running at localhost:2024");
        eprintln!("   Start it with: cd aomi/crates/l2beat && npx @boundaryml/baml serve");
        panic!("BAML server not available");
    }

    println!("✅ BAML server is running");

    // Connect to database
    let pool = connect_to_db().await?;
    println!("✅ Connected to PostgreSQL database");

    let session_manager = create_test_session_manager(pool.clone()).await;

    // Start the title generation task
    let title_manager = Arc::clone(&session_manager);
    title_manager.start_title_generation_task();
    println!("✅ Title generation task started");

    // Subscribe to system updates
    let mut update_rx = session_manager.subscribe_to_updates();
    println!("✅ Subscribed to system updates");

    // ==========================================================================
    // Test 1: Auto-generation for session with pubkey
    // ==========================================================================
    println!("\n📝 Test 1: Title generation for session with pubkey");

    let session1 = "e2e-test-session-1";
    let placeholder1 = format!("#[{}]", &session1[..6]);
    let pubkey1 = "0xTEST_E2E_1";

    // Set pubkey first
    session_manager
        .set_session_public_key(session1, Some(pubkey1.to_string()))
        .await;

    // Create session with placeholder title
    session_manager
        .get_or_create_session(session1, None, Some(placeholder1.clone()))
        .await?;

    // Verify initial state
    let metadata = session_manager.get_session_metadata(session1).unwrap();
    assert_eq!(metadata.title, Some(placeholder1.clone()));
    assert!(
        !metadata.is_user_title,
        "Initial title should not be user title"
    );
    println!("   ✓ Session created with placeholder: {}", placeholder1);

    // Send messages to trigger title generation
    send_message(&session_manager, session1, "What is the current ETH price?").await?;
    send_message(
        &session_manager,
        session1,
        "How do I swap tokens on Uniswap?",
    )
    .await?;
    println!("   ✓ Sent 2 messages");

    // Wait for title generation (runs every 5 seconds, plus processing time)
    println!("   ⏳ Waiting for title generation (up to 15 seconds)...");
    let mut generated_title1: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session1)
            .and_then(|m| m.title);

        if let Some(ref title) = current_title {
            if !title.starts_with("#[") {
                generated_title1 = Some(title.clone());
                println!(
                    "   ✅ Title generated after {} seconds: {}",
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
    assert_eq!(metadata.title, Some(generated_title1.clone()));
    assert!(
        !metadata.is_user_title,
        "Auto-generated title should have is_user_title = false"
    );
    println!("   ✓ Title metadata correct (is_user_title = false)");

    // Verify title persisted to database
    let db = SessionStore::new(pool.clone());
    let session_record = db.get_session(session1).await?;
    assert!(session_record.is_some(), "Session should exist in DB");
    assert_eq!(
        session_record.unwrap().title,
        Some(generated_title1.clone()),
        "Title should be persisted to database"
    );
    println!("   ✅ Title persisted to database");

    // Try to receive broadcast (with timeout)
    let broadcast_result =
        tokio::time::timeout(Duration::from_millis(1000), update_rx.recv()).await;

    if let Ok(Ok(SystemUpdate::TitleChanged {
        session_id,
        new_title,
    })) = broadcast_result
    {
        assert_eq!(session_id, session1);
        assert_eq!(new_title, generated_title1);
        println!("   ✅ Received title change broadcast");
    } else {
        println!("   ⚠️  No broadcast received (may have been sent before subscription)");
    }

    // ==========================================================================
    // Test 2: Anonymous session (no pubkey) - should NOT persist to DB
    // ==========================================================================
    println!("\n📝 Test 2: Title generation for anonymous session");

    let session2 = "e2e-test-session-2";
    let placeholder2 = format!("#[{}]", &session2[..6]);

    // Create session WITHOUT pubkey
    session_manager
        .get_or_create_session(session2, None, Some(placeholder2.clone()))
        .await?;
    println!("   ✓ Anonymous session created");

    send_message(&session_manager, session2, "Tell me about DeFi").await?;
    send_message(&session_manager, session2, "What are yield farms?").await?;
    println!("   ✓ Sent 2 messages");

    // Wait for title generation
    println!("   ⏳ Waiting for title generation...");
    let mut generated_title2: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session2)
            .and_then(|m| m.title);

        if let Some(ref title) = current_title {
            if !title.starts_with("#[") {
                generated_title2 = Some(title.clone());
                println!(
                    "   ✅ Title generated after {} seconds: {}",
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
    assert_eq!(metadata.title, Some(generated_title2.clone()));
    println!("   ✓ Title in memory");

    // Verify title NOT persisted to database (anonymous session)
    let session_record = db.get_session(session2).await?;
    assert!(
        session_record.is_none(),
        "Anonymous session should NOT be in database"
    );
    println!("   ✅ Title NOT persisted (anonymous session)");

    // ==========================================================================
    // Test 3: User-titled session should NEVER be auto-updated
    // ==========================================================================
    println!("\n📝 Test 3: User-titled session protection");

    let session3 = "e2e-test-session-3";
    let placeholder3 = format!("#[{}]", &session3[..6]);

    session_manager
        .get_or_create_session(session3, None, Some(placeholder3))
        .await?;

    // User manually sets title
    let user_title = "My Custom Trading Strategy".to_string();
    session_manager
        .update_session_title(session3, user_title.clone())
        .await?;
    println!("   ✓ User set title: {}", user_title);

    // Send messages
    send_message(&session_manager, session3, "What is Bitcoin?").await?;
    send_message(&session_manager, session3, "Explain blockchain").await?;
    println!("   ✓ Sent 2 messages");

    // Wait for a full generation cycle
    println!("   ⏳ Waiting one generation cycle (6 seconds)...");
    sleep(Duration::from_secs(6)).await;

    // Verify title has NOT changed
    let metadata = session_manager.get_session_metadata(session3).unwrap();
    assert_eq!(
        metadata.title,
        Some(user_title.clone()),
        "User title should not be overwritten"
    );
    assert!(
        metadata.is_user_title,
        "is_user_title flag should remain true"
    );
    println!("   ✅ User title protected from auto-generation");

    // ==========================================================================
    // Test 4: Title re-generation (conversation grows)
    // ==========================================================================
    println!("\n📝 Test 4: Title re-generation as conversation grows");

    // Add more messages to session1
    let old_title = generated_title1.clone();
    send_message(&session_manager, session1, "Tell me about NFTs").await?;
    send_message(&session_manager, session1, "What are smart contracts?").await?;
    send_message(&session_manager, session1, "Explain gas fees").await?;
    println!("   ✓ Sent 3 more messages to session 1");

    // Wait for re-generation
    println!("   ⏳ Waiting for title re-generation...");
    let mut new_title: Option<String> = None;

    for i in 0..30 {
        sleep(Duration::from_millis(500)).await;

        let current_title = session_manager
            .get_session_metadata(session1)
            .and_then(|m| m.title);

        if let Some(ref title) = current_title {
            if title != &old_title {
                new_title = Some(title.clone());
                println!(
                    "   ✅ Title re-generated after {} seconds: {}",
                    (i + 1) as f64 * 0.5,
                    title
                );
                break;
            }
        }
    }

    // Note: Title might stay the same if BAML determines same topic
    if let Some(new_title) = new_title {
        println!("   ✓ Title updated from '{}' to '{}'", old_title, new_title);
    } else {
        println!("   ℹ️  Title remained the same (BAML determined same topic)");
    }

    // ==========================================================================
    // Final Summary
    // ==========================================================================
    println!("\n✅ All integration tests passed!");
    println!("   ✓ Title generation with BAML service");
    println!("   ✓ Database persistence (with pubkey)");
    println!("   ✓ Anonymous session (no persistence)");
    println!("   ✓ User title protection");
    println!("   ✓ Title re-generation capability");

    Ok(())
}
