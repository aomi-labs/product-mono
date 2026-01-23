use std::{sync::Arc, time::Duration};

use crate::events::{SystemEvent, SystemEventQueue};
use aomi_mcp::client::{MCP_TOOLBOX, McpToolBox};
use aomi_rag::DocumentStore;
use aomi_tools::docs::SharedDocuments;
use eyre::Result;
use rig::agent::Agent;
use tokio::sync::Mutex;

/// Attempt to obtain the toolbox with retry feedback for the UI path.
pub async fn toolbox_with_retry(system_events: &SystemEventQueue) -> Result<Arc<McpToolBox>> {
    if let Some(existing) = MCP_TOOLBOX.get() {
        return Ok(existing.clone());
    }

    let mut attempt = 1;
    let max_attempts = 12; // About 1 minute of retries
    let mut delay = std::time::Duration::from_millis(500);

    loop {
        system_events.push(SystemEvent::SystemNotice("Backend connecting".into()));

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

                system_events.push(SystemEvent::SystemNotice("Backend connected".into()));
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

                system_events.push(SystemEvent::SystemNotice("Backend connecting".into()));

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
    system_events: &SystemEventQueue,
) -> Result<()> {
    let mut delay = Duration::from_millis(500);

    for attempt in 1..=3 {
        if attempt == 1 {
            system_events.push(SystemEvent::SystemNotice("Backend connecting".into()));
        }

        match test_model_connection(agent).await {
            Ok(()) => {
                system_events.push(SystemEvent::SystemNotice("Backend connected".into()));
                system_events.push(SystemEvent::SystemNotice(
                    "✓ Model API connection successful".into(),
                ));
                return Ok(());
            }
            Err(e) if attempt == 3 => {
                system_events.push(SystemEvent::SystemError(format!("Failed to connect to model API after 3 attempts: {e}. Please check your API key and connection.")));
                return Err(e);
            }
            Err(_) => {
                system_events.push(SystemEvent::SystemNotice("Backend connecting".into()));
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(5));
            }
        }
    }
    unreachable!()
}

pub async fn init_document_store() -> Result<SharedDocuments> {
    let mut store = DocumentStore::new().await?;

    // Load all documentation directories
    store
        .load_directory("documents/concepts", 1000, 100)
        .await?;
    store
        .load_directory("documents/contracts/v2", 1000, 100)
        .await?;
    store
        .load_directory("documents/contracts/v3", 1000, 100)
        .await?;

    // Load Solidity contract files
    store
        .load_directory("documents/v2-contracts", 1500, 150)
        .await?;
    store
        .load_directory("documents/v3-contracts", 1500, 150)
        .await?;
    store
        .load_directory("documents/swap-router-contracts", 1500, 150)
        .await?;

    Ok(SharedDocuments::new(Arc::new(Mutex::new(store))))
}
