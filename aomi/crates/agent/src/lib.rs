// Re-export rig providers for convenience
pub use rig::providers;

// Internal modules
mod accounts;
mod agent;
mod completion;
mod docs;
mod mcp;
mod tool_scheduler;
mod types;

// Public re-exports
pub use agent::*;
pub use completion::{RespondStream, stream_completion};
pub use docs::{LoadingProgress, SharedDocuments, initialize_document_store_with_progress};
pub use rig::message::{AssistantContent, Message, UserContent};
pub use tool_scheduler::*;
pub use types::*;

// Re-export tooling from the shared tools crate for backwards compatibility
pub use aomi_tools::{
    Contract, ContractStore, ContractStoreApi, EncodeFunctionCall, EncodeFunctionCallParameters,
    GetCurrentTime, GetCurrentTimeParameters, SendTransactionToWallet,
    SendTransactionToWalletParameters, Transaction, abi_encoder, db, db_tools,
    get_contract_info, store_contract_info, etherscan, time, wallet,
};

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore] // Test when MCP server is running
    async fn test_mcp_connection() {
        let url = crate::mcp::server_url();
        println!("MCP URL: {}", url);

        let toolbox = crate::mcp::toolbox().await.unwrap();
        toolbox.ensure_connected().await.unwrap();

        let tools = toolbox.tools();
        println!("Tools: {:?}", tools);
    }
}
