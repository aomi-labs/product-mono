use std::{sync::Arc, time::Duration};

use crate::app::LoadingProgress;
use crate::{SystemEvent, SystemEventQueue};
use aomi_mcp::client::{MCP_TOOLBOX, McpToolBox};
use aomi_rag::DocumentStore;
use aomi_tools::docs::SharedDocuments;
use eyre::Result;
use rig::agent::Agent;
use tokio::sync::{Mutex, mpsc};

/// Attempt to obtain the toolbox with retry feedback for the UI path.
pub async fn toolbox_with_retry(
    _sender_to_ui: mpsc::Sender<crate::app::ChatCommand>,
    system_events: SystemEventQueue,
) -> Result<Arc<McpToolBox>> {
    if let Some(existing) = MCP_TOOLBOX.get() {
        return Ok(existing.clone());
    }

    let mut attempt = 1;
    let max_attempts = 12; // About 1 minute of retries
    let mut delay = std::time::Duration::from_millis(500);

    loop {
        system_events.push(SystemEvent::BackendConnecting(format!(
            "Connecting to MCP server (attempt {attempt}/{max_attempts})"
        )));

        match McpToolBox::connect().await {
            Ok(toolbox) => {
                if let Err(e) = toolbox.ensure_connected().await {
                    system_events.push(SystemEvent::SystemError(format!(
                        "MCP connection failed validation: {e}"
                    )));
                    return Err(e);
                }

                let arc = Arc::new(toolbox);
                if MCP_TOOLBOX.set(arc.clone()).is_err()
                    && let Some(existing) = MCP_TOOLBOX.get()
                {
                    return Ok(existing.clone());
                }

                system_events.push(SystemEvent::SystemNotice(
                    "✓ MCP server connection successful".to_string(),
                ));
                return Ok(arc);
            }
            Err(e) => {
                if attempt >= max_attempts {
                    let mcp_url = aomi_mcp::server_url();
                    system_events.push(SystemEvent::SystemError(format!(
                        "Failed to connect to MCP server after {max_attempts} attempts: {e}. Please make sure it's running at {mcp_url}"
                    )));
                    return Err(e);
                }

                system_events.push(SystemEvent::BackendConnecting(format!(
                    "Connection failed, retrying in {:.1}s...",
                    delay.as_secs_f32()
                )));

                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay * 2, std::time::Duration::from_secs(5)); // Max 5 second delay
                attempt += 1;
            }
        }
    }
}

async fn test_model_connection<M: rig::completion::CompletionModel>(
    agent: &Arc<Agent<M>>,
) -> Result<()> {
    use rig::completion::Prompt;

    let test_prompt = "Say 'Connection test successful' and nothing else.";

    match agent.prompt(test_prompt).await {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub async fn ensure_connection_with_retries<M: rig::completion::CompletionModel>(
    agent: &Arc<Agent<M>>,
    _sender_to_ui: &mpsc::Sender<crate::app::ChatCommand>,
    system_events: SystemEventQueue,
) -> Result<()> {
    let mut delay = Duration::from_millis(500);

    for attempt in 1..=3 {
        if attempt == 1 {
            system_events.push(SystemEvent::BackendConnecting(
                "Testing connection to Anthropic API...".into(),
            ));
        }

        match test_model_connection(agent).await {
            Ok(()) => {
                system_events.push(SystemEvent::SystemNotice(
                    "✓ Anthropic API connection successful".into(),
                ));
                system_events.push(SystemEvent::BackendConnected);
                return Ok(());
            }
            Err(e) if attempt == 3 => {
                system_events.push(SystemEvent::SystemError(format!("Failed to connect to Anthropic API after 3 attempts: {e}. Please check your API key and connection.")));
                return Err(e);
            }
            Err(_) => {
                system_events.push(SystemEvent::BackendConnecting(format!(
                    "Connection failed, retrying in {:.1}s...",
                    delay.as_secs_f32()
                )));
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
        }
    }
    unreachable!()
}

pub async fn init_document_store(
    progress_sender: Option<mpsc::Sender<LoadingProgress>>,
) -> Result<SharedDocuments> {
    // Helper function to send progress
    async fn send_progress(msg: String, sender: &Option<mpsc::Sender<LoadingProgress>>) {
        if let Some(sender) = sender {
            let _ = sender.send(LoadingProgress::Message(msg)).await;
        } else {
            println!("{msg}");
        }
    }

    send_progress(
        "Loading Uniswap documentation...".to_string(),
        &progress_sender,
    )
    .await;
    let mut store = DocumentStore::new().await?;

    // Load all documentation directories
    let concepts_count = store
        .load_directory("documents/concepts", 1000, 100)
        .await?;
    send_progress(
        format!("  Loaded {concepts_count} chunks from concepts"),
        &progress_sender,
    )
    .await;

    let v2_docs_count = store
        .load_directory("documents/contracts/v2", 1000, 100)
        .await?;
    send_progress(
        format!("  Loaded {v2_docs_count} chunks from V2 docs"),
        &progress_sender,
    )
    .await;

    let v3_docs_count = store
        .load_directory("documents/contracts/v3", 1000, 100)
        .await?;
    send_progress(
        format!("  Loaded {v3_docs_count} chunks from V3 docs"),
        &progress_sender,
    )
    .await;

    // Load Solidity contract files
    let v2_contracts_count = store
        .load_directory("documents/v2-contracts", 1500, 150)
        .await?;
    send_progress(
        format!("  Loaded {v2_contracts_count} chunks from V2 contracts"),
        &progress_sender,
    )
    .await;

    let v3_contracts_count = store
        .load_directory("documents/v3-contracts", 1500, 150)
        .await?;
    send_progress(
        format!("  Loaded {v3_contracts_count} chunks from V3 contracts"),
        &progress_sender,
    )
    .await;

    let swap_router_count = store
        .load_directory("documents/swap-router-contracts", 1500, 150)
        .await?;
    send_progress(
        format!("  Loaded {swap_router_count} chunks from Swap Router contracts"),
        &progress_sender,
    )
    .await;

    send_progress(
        format!("Total document chunks indexed: {}", store.document_count()),
        &progress_sender,
    )
    .await;

    if let Some(sender) = progress_sender {
        let _ = sender.send(LoadingProgress::Complete).await;
    }

    Ok(SharedDocuments::new(Arc::new(Mutex::new(store))))
}
