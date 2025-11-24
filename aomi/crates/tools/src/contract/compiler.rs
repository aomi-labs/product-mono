use anyhow::Result;
use foundry_compilers::{artifacts::{Source, Sources, ConfigurableContractArtifact}, compilers::solc::Solc, project::ProjectCompiler, Artifact, ProjectCompileOutput};
use foundry_config::{Config, SolcReq};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for Solidity compilation
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CompilerConfig {
    /// Foundry configuration
    pub foundry_config: Config,
    /// Disable automatic solc version detection
    pub no_auto_detect: bool,
}

impl CompilerConfig {
    /// Detect and set the solc version if not already configured
    pub fn detect_solc(&mut self) -> Result<()> {
        if self.foundry_config.solc.is_none() && !self.no_auto_detect {
            let version = Solc::ensure_installed(&"*".parse().unwrap())?;
            self.foundry_config.solc = Some(SolcReq::Version(version));
        }
        Ok(())
    }
}

/// A Solidity contract compiler that wraps foundry's compilation capabilities
pub struct ContractCompiler {
    config: CompilerConfig,
}

impl ContractCompiler {
    /// Create a new contract compiler with the given configuration
    pub fn new(mut config: CompilerConfig) -> Result<Self> {
        config.detect_solc()?;
        Ok(Self { config })
    }

    /// Create a new contract compiler with default configuration
    pub fn default() -> Result<Self> {
        Self::new(CompilerConfig::default())
    }

    /// Compile a single Solidity source file
    pub fn compile_source(&self, source_path: PathBuf, content: String) -> Result<ProjectCompileOutput> {
        let mut sources = Sources::new();
        sources.insert(source_path.into(), Source::new(content));
        self.compile_sources(sources)
    }

    /// Compile multiple Solidity source files
    pub fn compile_sources(&self, sources: Sources) -> Result<ProjectCompileOutput> {
        let project = self.config.foundry_config.ephemeral_project()?;
        let output = ProjectCompiler::with_sources(&project, sources)?.compile()?;

        if output.has_compiler_errors() {
            anyhow::bail!("Compilation failed:\n{}", output);
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
        let contract = output
            .find_first(contract_name)
            .ok_or_else(|| anyhow::anyhow!("Contract '{}' not found in compilation output", contract_name))?;

        let bytecode = contract
            .get_bytecode_bytes()
            .ok_or_else(|| anyhow::anyhow!("No bytecode found for contract '{}'", contract_name))?;

        Ok(Vec::from(bytecode.into_owned()))
    }

    /// Get the deployed bytecode for a specific contract from compilation output
    pub fn get_contract_deployed_bytecode(
        &self,
        output: &ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<Vec<u8>> {
        let contract = output
            .find_first(contract_name)
            .ok_or_else(|| anyhow::anyhow!("Contract '{}' not found in compilation output", contract_name))?;

        let deployed_bytecode = contract
            .get_deployed_bytecode()
            .ok_or_else(|| anyhow::anyhow!("No deployed bytecode found for contract '{}'", contract_name))?;

        let bytecode_bytes = deployed_bytecode
            .bytes()
            .ok_or_else(|| anyhow::anyhow!("No deployed bytecode bytes found for contract '{}'", contract_name))?;

        Ok(bytecode_bytes.to_vec())
    }

    /// Get a contract artifact by name from compilation output
    pub fn get_contract_artifact<'a>(
        &self,
        output: &'a ProjectCompileOutput,
        contract_name: &str,
    ) -> Result<&'a ConfigurableContractArtifact> {
        output
            .find_first(contract_name)
            .ok_or_else(|| anyhow::anyhow!("Contract '{}' not found in compilation output", contract_name))
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
                Err(e) => Err(anyhow::anyhow!("Failed to serialize ABI for contract '{}': {}", contract_name, e))
            }
        } else {
            Err(anyhow::anyhow!("No ABI found for contract '{}'", contract_name))
        }
    }
}

#[cfg(all(test, feature = "contract-tests"))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compiler_creation() {
        let compiler = ContractCompiler::default();
        assert!(compiler.is_ok());
    }

    #[test]
    fn test_simple_contract_compilation() {
        let compiler = ContractCompiler::default().unwrap();

        let simple_contract = r#"
        // SPDX-License-Identifier: MIT
        pragma solidity ^0.8.0;

        contract SimpleStorage {
            uint256 public value;

            function setValue(uint256 _value) public {
                value = _value;
            }

            function getValue() public view returns (uint256) {
                return value;
            }
        }
        "#;

        let result = compiler.compile_source(
            PathBuf::from("SimpleStorage.sol"),
            simple_contract.to_string(),
        );

        assert!(result.is_ok());

        let output = result.unwrap();
        let bytecode = compiler.get_contract_bytecode(&output, "SimpleStorage");
        assert!(bytecode.is_ok());
        assert!(!bytecode.unwrap().is_empty());
    }
}