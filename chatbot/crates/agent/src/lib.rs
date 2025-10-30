// Re-export rig providers for convenience
pub use rig::providers;

macro_rules! impl_rig_tool_clone {
    ($tool:ident, $params:ident, []) => {
        impl Clone for $tool {
            fn clone(&self) -> Self {
                Self
            }
        }

        impl Clone for $params {
            fn clone(&self) -> Self {
                Self {}
            }
        }
    };
    ($tool:ident, $params:ident, [$($field:ident),+ $(,)?]) => {
        impl Clone for $tool {
            fn clone(&self) -> Self {
                Self
            }
        }

        impl Clone for $params {
            fn clone(&self) -> Self {
                Self {
                    $( $field: self.$field.clone(), )*
                }
            }
        }
    };
}

// Internal modules
mod accounts;
mod agent;
mod completion;
mod mcp;
mod tool_scheduler;
mod types;
pub mod tools;

// Public re-exports
pub use agent::*;
pub use completion::{RespondStream, stream_completion};
pub use tools::docs::{LoadingProgress, initialize_document_store_with_progress};
pub use rig::message::{AssistantContent, Message, UserContent};
pub use tool_scheduler::*;
pub use types::*;

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
