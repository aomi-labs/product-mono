// Re-export BAML generated types for convenience
pub use super::baml_client::models::{
    CodeLine, ContractInfo, Event, ExtractContractInfoRequest, ExtractedContractInfo, Function,
    GenerateScriptRequest, Import, Interface, ScriptBlock, Storage,
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
