// BAML client module for Forge script generation
//
// This module provides a two-phase interface for generating Forge scripts:
// 1. Phase 1: Extract relevant contract information from full ABIs and source code
// 2. Phase 2: Generate Solidity script code with import/interface decisions
//
// Uses native BAML FFI runtime (no HTTP server needed)

// Generated native BAML client (via baml-cli generate)
#[path = "../baml_client/mod.rs"]
pub mod baml_client;

pub mod client;
pub mod types;

// Re-export main types for convenience
pub use client::BamlClient;
pub use types::{
    CodeLine, ContractInfo, ContractSource, Event, ExtractedContractInfo, Function,
    Import, Interface, ScriptBlock, Storage,
};

// Re-export the async client for direct access to all BAML functions
pub use baml_client::async_client;
