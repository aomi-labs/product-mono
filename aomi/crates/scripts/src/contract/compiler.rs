use eyre::Result;
use foundry_compilers::{
    Artifact, ProjectCompileOutput,
    artifacts::{ConfigurableContractArtifact, Source, Sources},
    compilers::solc::Solc,
    project::ProjectCompiler,
};
use foundry_config::SolcReq;
use std::path::PathBuf;
use std::sync::Arc;

use super::session::ContractConfig;

/// A Solidity contract compiler that wraps foundry's compilation capabilities
pub struct ContractCompiler {
    /// Reference to the contract configuration (potentially modified for solc auto-detection)
    pub config: Arc<super::session::ContractConfig>,
}

impl ContractCompiler {
    /// Create a new contract compiler with the given configuration
    pub fn new(config: &ContractConfig) -> Result<Self> {
        // If we need to auto-detect solc, we need to modify the config
        let config_arc = if config.foundry_config.solc.is_none() && !config.no_auto_detect {
            let mut modified_config = config.clone();
            let mut modified_foundry = (*config.foundry_config).clone();
            let version = Solc::ensure_installed(&"*".parse().unwrap())?;
            modified_foundry.solc = Some(SolcReq::Version(version));
            modified_config.foundry_config = Arc::new(modified_foundry);
            Arc::new(modified_config)
        } else {
            // Otherwise, just wrap the cloned config in Arc
            Arc::new(config.clone())
        };

        Ok(Self { config: config_arc })
    }

    /// Compile a single Solidity source file
    pub fn compile_source(
        &self,
        source_path: PathBuf,
        content: String,
    ) -> Result<ProjectCompileOutput> {
        let mut sources = Sources::new();
        sources.insert(source_path, Source::new(content));
        self.compile_sources(sources)
    }

    /// Compile multiple Solidity source files
    pub fn compile_sources(&self, sources: Sources) -> Result<ProjectCompileOutput> {
        let project = self.config.foundry_config.ephemeral_project()?;
        let output = ProjectCompiler::with_sources(&project, sources)?.compile()?;

        if output.has_compiler_errors() {
            eyre::bail!("Compilation failed:\n{}", output);
        }

        Ok(output)
    }

    /// Compile a contract from a file path
    pub fn compile_file(&self, file_path: PathBuf) -> Result<ProjectCompileOutput> {
        let content = std::fs::read_to_string(&file_path)?;
        self.compile_source(file_path, content)
    }

    /// Get the bytecode for a specific contract from compilation output
    pub fn get_contract_bytecode(
        &self,
        output: &ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<Vec<u8>> {
        let contract = output.find_first(contract_name).ok_or_else(|| {
            eyre::eyre!(
                "Contract '{}' not found in compilation output",
                contract_name
            )
        })?;

        let bytecode = contract
            .get_bytecode_bytes()
            .ok_or_else(|| eyre::eyre!("No bytecode found for contract '{}'", contract_name))?;

        Ok(Vec::from(bytecode.into_owned()))
    }

    /// Get the deployed bytecode for a specific contract from compilation output
    pub fn get_contract_deployed_bytecode(
        &self,
        output: &ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<Vec<u8>> {
        let contract = output.find_first(contract_name).ok_or_else(|| {
            eyre::eyre!(
                "Contract '{}' not found in compilation output",
                contract_name
            )
        })?;

        let deployed_bytecode = contract.get_deployed_bytecode().ok_or_else(|| {
            eyre::eyre!(
                "No deployed bytecode found for contract '{}'",
                contract_name
            )
        })?;

        let bytecode_bytes = deployed_bytecode.bytes().ok_or_else(|| {
            eyre::eyre!(
                "No deployed bytecode bytes found for contract '{}'",
                contract_name
            )
        })?;

        Ok(bytecode_bytes.to_vec())
    }

    /// Get a contract artifact by name from compilation output
    pub fn get_contract_artifact<'a>(
        &self,
        output: &'a ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<&'a ConfigurableContractArtifact> {
        output.find_first(contract_name).ok_or_else(|| {
            eyre::eyre!(
                "Contract '{}' not found in compilation output",
                contract_name
            )
        })
    }

    /// Get the ABI for a specific contract from compilation output
    pub fn get_contract_abi(
        &self,
        output: &ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<String> {
        let artifact = self.get_contract_artifact(output, contract_name)?;

        if let Some(abi) = &artifact.abi {
            match serde_json::to_string(abi) {
                Ok(abi_json) => Ok(abi_json),
                Err(e) => Err(eyre::eyre!(
                    "Failed to serialize ABI for contract '{}': {}",
                    contract_name,
                    e
                )),
            }
        } else {
            Err(eyre::eyre!(
                "No ABI found for contract '{}'",
                contract_name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn constant_contract() -> (PathBuf, String) {
        let path = PathBuf::from("Constant.sol");
        let source = r#"
            // SPDX-License-Identifier: UNLICENSED
            pragma solidity ^0.8.20;

            contract Constant {
                function value() external pure returns (uint256) {
                    return 42;
                }
            }
        "#
        .to_string();
        (path, source)
    }

    #[test]
    fn bytecode_is_returned_when_present() {
        let compiler = ContractCompiler::new(&ContractConfig::default()).expect("compiler init");
        let (path, source) = constant_contract();
        let output = compiler
            .compile_source(path, source)
            .expect("compile succeeds");

        let result = compiler
            .get_contract_bytecode(&output, "Constant")
            .expect("bytecode exists");
        assert!(!result.is_empty());
    }

    #[test]
    fn deployed_bytecode_is_returned_when_present() {
        let compiler = ContractCompiler::new(&ContractConfig::default()).expect("compiler init");
        let (path, source) = constant_contract();
        let output = compiler
            .compile_source(path, source)
            .expect("compile succeeds");

        let result = compiler
            .get_contract_deployed_bytecode(&output, "Constant")
            .expect("deployed bytecode exists");
        assert!(!result.is_empty());
    }

    #[test]
    fn abi_serializes_to_json() {
        let compiler = ContractCompiler::new(&ContractConfig::default()).expect("compiler init");
        let (path, source) = constant_contract();
        let output = compiler
            .compile_source(path, source)
            .expect("compile succeeds");

        let abi = compiler
            .get_contract_abi(&output, "Constant")
            .expect("ABI exists");
        assert!(abi.contains("\"value\""));
    }

    #[test]
    fn errors_when_contract_missing() {
        let compiler = ContractCompiler::new(&ContractConfig::default()).expect("compiler init");
        let (path, source) = constant_contract();
        let output = compiler
            .compile_source(path, source)
            .expect("compile succeeds");

        let err = compiler
            .get_contract_bytecode(&output, "Missing")
            .expect_err("expected missing contract error");
        assert!(err.to_string().contains("not found"));
    }
}
