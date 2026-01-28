// Re-export BAML generated types for convenience
pub use crate::baml_client::types::{
    // Additional types that may be useful
    ABIAnalysisResult,
    CodeLine,
    ContractAnalysis,
    ContractInfo,
    ConversationSummary,
    Event,
    EventAnalyzeResult,
    ExtractedContractInfo,
    Function,
    GeneratedScript,
    Import,
    Interface,
    LayoutAnalysisResult,
    ScriptBlock,
    SessionTitle,
    Storage,
    TransactionCall,
};

/// Contract source information fetched from DB or Etherscan
#[derive(Clone, Debug)]
pub struct ContractSource {
    pub chain_id: String,
    pub address: String,
    pub name: String,
    pub abi: String,
    pub source_code: Option<String>,
}

impl From<&ContractSource> for ContractInfo {
    fn from(source: &ContractSource) -> Self {
        ContractInfo {
            description: Some(source.name.clone()),
            address: source.address.clone(),
            abi: source.abi.clone(),
            source_code: source.source_code.clone(),
        }
    }
}

// Re-export the request type alias for backwards compatibility
pub type GenerateScriptRequest = (Vec<String>, Vec<ExtractedContractInfo>);
