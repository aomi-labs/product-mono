use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileSessionArgs {
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileSessionResult {
    pub success: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CompileSession;

impl Tool for CompileSession {
    const NAME: &'static str = "compile_session";
    type Args = CompileSessionArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Compile the current script session and return compile errors.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Solidity source code to compile"
                    }
                },
                "required": ["source"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = CompileSessionResult {
            success: false,
            errors: vec!["compile_session not wired yet".to_string()],
        };
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteContractArgs {
    pub chain_id: u64,
    pub address: String,
    pub calldata: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteContractResult {
    pub success: bool,
    pub return_data: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecuteContract;

impl Tool for ExecuteContract {
    const NAME: &'static str = "execute_contract";
    type Args = ExecuteContractArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a contract call and return the result.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "chain_id": {
                        "type": "number",
                        "description": "Numeric EVM chain ID"
                    },
                    "address": {
                        "type": "string",
                        "description": "Target contract address"
                    },
                    "calldata": {
                        "type": "string",
                        "description": "Hex calldata for the call"
                    },
                    "value": {
                        "type": "string",
                        "description": "Hex value to send (optional)"
                    }
                },
                "required": ["chain_id", "address", "calldata"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = ExecuteContractResult {
            success: false,
            return_data: "0x".to_string(),
            error: Some("execute_contract not wired yet".to_string()),
        };
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditScriptArgs {
    pub source: String,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditScriptResult {
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct EditScript;

impl Tool for EditScript {
    const NAME: &'static str = "edit_script";
    type Args = EditScriptArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Edit the Solidity script based on instructions.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Solidity source to edit"
                    },
                    "instructions": {
                        "type": "string",
                        "description": "Instructions describing the desired edits"
                    }
                },
                "required": ["source", "instructions"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = EditScriptResult { source: args.source };
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchContractArgs {
    pub chain_id: u64,
    pub address: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchContractResult {
    pub success: bool,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct FetchContract;

impl Tool for FetchContract {
    const NAME: &'static str = "fetch_contract";
    type Args = FetchContractArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch contract sources for dependencies.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "chain_id": {
                        "type": "number",
                        "description": "Numeric EVM chain ID"
                    },
                    "address": {
                        "type": "string",
                        "description": "Target contract address"
                    },
                    "name": {
                        "type": "string",
                        "description": "Contract name hint"
                    }
                },
                "required": ["chain_id", "address"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = FetchContractResult {
            success: false,
            source: String::new(),
        };
        // TODO: implement
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocsArgs {
    pub topic: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocsResult {
    pub notice: String,
}

#[derive(Debug, Clone)]
pub struct SearchDocs;

impl Tool for SearchDocs {
    const NAME: &'static str = "search_docs";
    type Args = SearchDocsArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search documentation for relevant guidance.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on why docs are needed"
                    },
                    "query": {
                        "type": "string",
                        "description": "Docs search query"
                    }
                },
                "required": ["topic", "query"]
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = SearchDocsResult {
            notice: "search_docs is a stub; use search_uniswap_docs via the docs tool."
                .to_string(),
        };
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ToolCallError(format!("Serialization error: {}", e).into()))
    }
}
