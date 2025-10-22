use alloy_primitives::{Address, B256, hex, keccak256};
use alloy_provider::{Provider, RootProvider, network::Network};
use alloy_rpc_types::Log;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use super::config::HandlerDefinition;
use super::types::{Handler, HandlerResult, HandlerValue};

/// AccessControlHandler processes OpenZeppelin AccessControl events
/// to reconstruct the current state of roles and their members
#[derive(Debug, Clone)]
pub struct AccessControlHandler<N: Network> {
    field: String,
    role_names: HashMap<String, String>,
    pick_role_members: Option<String>,
    hidden: bool,
    _phantom: PhantomData<N>,
}

/// Represents the state of a single role
#[derive(Debug, Clone)]
struct RoleState {
    admin_role: B256,
    members: HashSet<Address>,
}

impl<N: Network> AccessControlHandler<N> {
    /// OpenZeppelin AccessControl event signatures (hardcoded)
    const ROLE_GRANTED_SIG: &'static str = "RoleGranted(bytes32,address,address)";
    const ROLE_REVOKED_SIG: &'static str = "RoleRevoked(bytes32,address,address)";
    const ROLE_ADMIN_CHANGED_SIG: &'static str = "RoleAdminChanged(bytes32,bytes32,bytes32)";

    /// Default admin role (bytes32(0))
    const DEFAULT_ADMIN_ROLE: B256 = B256::ZERO;

    pub fn new(
        field: String,
        role_names: HashMap<String, String>,
        pick_role_members: Option<String>,
        hidden: bool,
    ) -> Self {
        Self {
            field,
            role_names,
            pick_role_members,
            hidden,
            _phantom: PhantomData,
        }
    }

    /// Create AccessControlHandler from HandlerDefinition
    pub fn from_handler_definition(field: String, definition: HandlerDefinition) -> Result<Self> {
        match definition {
            HandlerDefinition::AccessControl {
                role_names,
                pick_role_members,
                ignore_relative,
                ..
            } => Ok(Self::new(
                field,
                role_names.unwrap_or_default(),
                pick_role_members,
                ignore_relative.unwrap_or(false),
            )),
            _ => Err(anyhow!("Expected AccessControl handler definition")),
        }
    }

    /// Compute topic0 hash for an event signature
    fn compute_topic0(event_sig: &str) -> B256 {
        keccak256(event_sig.as_bytes())
    }

    /// Fetch access control logs in batches to avoid RPC limitations
    async fn fetch_logs_batched(
        provider: &RootProvider<N>,
        address: &Address,
        topic0: B256,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<Log>> {
        const BATCH_SIZE: u64 = 1_000;
        let mut all_logs = Vec::new();
        let mut current_from = from_block;

        while current_from <= to_block {
            let current_to = (current_from + BATCH_SIZE - 1).min(to_block);

            let filter = alloy_rpc_types::Filter::new()
                .address(*address)
                .event_signature(topic0)
                .from_block(current_from)
                .to_block(current_to);

            match provider.get_logs(&filter).await {
                Ok(logs) => {
                    all_logs.extend(logs);
                }
                Err(e) => {
                    return Err(anyhow!(
                        "Failed to fetch logs for blocks {}-{}: {}",
                        current_from,
                        current_to,
                        e
                    ));
                }
            }

            current_from = current_to + 1;
        }

        Ok(all_logs)
    }

    /// Fetch all AccessControl events and reconstruct current state
    async fn fetch_access_control_state(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
    ) -> Result<HashMap<B256, RoleState>> {
        // Get current block number to determine range
        let to_block = provider
            .get_block_number()
            .await
            .map_err(|e| anyhow!("Failed to get current block number: {}", e))?;

        // Query last 10,000 blocks by default (to avoid timeout on full history)
        // For production, should query from contract deployment block
        let from_block = to_block.saturating_sub(10_000);

        // Compute topic0 for each event
        let role_granted_topic = Self::compute_topic0(Self::ROLE_GRANTED_SIG);
        let role_revoked_topic = Self::compute_topic0(Self::ROLE_REVOKED_SIG);
        let role_admin_changed_topic = Self::compute_topic0(Self::ROLE_ADMIN_CHANGED_SIG);

        // Fetch all logs
        let mut all_logs = Vec::new();

        all_logs.extend(
            Self::fetch_logs_batched(provider, address, role_granted_topic, from_block, to_block)
                .await?,
        );
        all_logs.extend(
            Self::fetch_logs_batched(provider, address, role_revoked_topic, from_block, to_block)
                .await?,
        );
        all_logs.extend(
            Self::fetch_logs_batched(
                provider,
                address,
                role_admin_changed_topic,
                from_block,
                to_block,
            )
            .await?,
        );

        // Sort logs chronologically
        all_logs.sort_by(|a, b| match a.block_number.cmp(&b.block_number) {
            std::cmp::Ordering::Equal => a.log_index.cmp(&b.log_index),
            other => other,
        });

        // Process logs to reconstruct state
        let mut roles: HashMap<B256, RoleState> = HashMap::new();

        for log in all_logs {
            let topics = log.topics();
            if topics.is_empty() {
                continue;
            }

            let topic0 = &topics[0];

            if topic0 == &role_granted_topic && topics.len() >= 3 {
                // RoleGranted(bytes32 indexed role, address indexed account, address indexed sender)
                let role = topics[1];
                let account = Self::topic_to_address(&topics[2])?;

                roles
                    .entry(role)
                    .or_insert_with(|| RoleState {
                        admin_role: Self::DEFAULT_ADMIN_ROLE,
                        members: HashSet::new(),
                    })
                    .members
                    .insert(account);
            } else if topic0 == &role_revoked_topic && topics.len() >= 3 {
                // RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender)
                let role = topics[1];
                let account = Self::topic_to_address(&topics[2])?;

                if let Some(role_state) = roles.get_mut(&role) {
                    role_state.members.remove(&account);
                }
            } else if topic0 == &role_admin_changed_topic && topics.len() >= 3 {
                // RoleAdminChanged(bytes32 indexed role, bytes32 indexed previousAdminRole, bytes32 indexed newAdminRole)
                let role = topics[1];
                let new_admin_role = topics[2];

                roles
                    .entry(role)
                    .or_insert_with(|| RoleState {
                        admin_role: new_admin_role,
                        members: HashSet::new(),
                    })
                    .admin_role = new_admin_role;
            }
        }

        Ok(roles)
    }

    /// Convert a topic (32 bytes) to an Address (20 bytes)
    fn topic_to_address(topic: &B256) -> Result<Address> {
        // Address is in the last 20 bytes of the topic
        let addr_bytes = &topic.as_slice()[12..32];
        Address::try_from(addr_bytes)
            .map_err(|e| anyhow!("Failed to parse address from topic: {}", e))
    }

    /// Convert B256 role hash to hex string
    fn role_to_string(role: &B256) -> String {
        format!("0x{}", hex::encode(role.as_slice()))
    }

    /// Get human-readable name for a role
    fn get_role_name(&self, role: &B256) -> String {
        let role_str = Self::role_to_string(role);

        // Check if it's the default admin role
        if role == &Self::DEFAULT_ADMIN_ROLE {
            return "DEFAULT_ADMIN_ROLE".to_string();
        }

        // Check user-provided role names
        if let Some(name) = self.role_names.get(&role_str) {
            return name.clone();
        }

        // Return the hash as-is if no name mapping found
        role_str
    }

    /// Convert role state to HandlerValue
    fn format_output(&self, roles: HashMap<B256, RoleState>) -> HandlerValue {
        // If picking specific role members, return just the members array
        if let Some(pick_role) = &self.pick_role_members {
            // Find the role by name
            for (role_hash, role_state) in roles.iter() {
                let role_name = self.get_role_name(role_hash);
                if role_name == *pick_role {
                    let members: Vec<HandlerValue> = role_state
                        .members
                        .iter()
                        .map(|addr| HandlerValue::Address(*addr))
                        .collect();
                    return HandlerValue::Array(members);
                }
            }
            // Role not found, return empty array
            return HandlerValue::Array(vec![]);
        }

        // Otherwise, return full role mapping
        let mut result = HashMap::new();

        for (role_hash, role_state) in roles {
            let role_name = self.get_role_name(&role_hash);

            let members: Vec<HandlerValue> = role_state
                .members
                .iter()
                .map(|addr| HandlerValue::Address(*addr))
                .collect();

            let mut role_data = HashMap::new();
            role_data.insert(
                "adminRole".to_string(),
                HandlerValue::String(self.get_role_name(&role_state.admin_role)),
            );
            role_data.insert("members".to_string(), HandlerValue::Array(members));

            result.insert(role_name, HandlerValue::Object(role_data));
        }

        HandlerValue::Object(result)
    }
}

#[async_trait]
impl<N: Network> Handler<N> for AccessControlHandler<N> {
    fn field(&self) -> &str {
        &self.field
    }

    fn dependencies(&self) -> &[String] {
        &[] // AccessControl doesn't depend on other fields
    }

    fn hidden(&self) -> bool {
        self.hidden
    }

    async fn execute(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        _previous_results: &HashMap<String, HandlerResult>,
    ) -> HandlerResult {
        // Fetch and reconstruct access control state
        let roles = match self.fetch_access_control_state(provider, address).await {
            Ok(roles) => roles,
            Err(e) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to fetch access control state: {}", e)),
                    hidden: self.hidden,
                };
            }
        };

        // Format output
        let value = self.format_output(roles);

        HandlerResult {
            field: self.field.clone(),
            value: Some(value),
            error: None,
            hidden: self.hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::network::AnyNetwork;

    type AnyAccessControlHandler = AccessControlHandler<AnyNetwork>;

    #[test]
    fn test_from_handler_definition() {
        let definition = HandlerDefinition::AccessControl {
            role_names: Some(HashMap::from([(
                "0x9f2df0fed2c77648de5860a4cc508cd0818c85b8b8a1ab4ceeef8d981c8956a6".to_string(),
                "MINTER_ROLE".to_string(),
            )])),
            pick_role_members: None,
            ignore_relative: Some(false),
            extra: None,
        };

        let handler = AnyAccessControlHandler::from_handler_definition(
            "accessControl".to_string(),
            definition,
        );

        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.field(), "accessControl");
        assert_eq!(handler.role_names.len(), 1);
    }

    #[test]
    fn test_from_handler_definition_pick_role() {
        let definition = HandlerDefinition::AccessControl {
            role_names: None,
            pick_role_members: Some("MINTER_ROLE".to_string()),
            ignore_relative: None,
            extra: None,
        };

        let handler =
            AnyAccessControlHandler::from_handler_definition("minters".to_string(), definition);

        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.pick_role_members, Some("MINTER_ROLE".to_string()));
    }

    #[test]
    fn test_compute_topic0() {
        // Test RoleGranted event signature
        let topic0 =
            AnyAccessControlHandler::compute_topic0(AnyAccessControlHandler::ROLE_GRANTED_SIG);

        // The expected topic0 for RoleGranted(bytes32,address,address)
        let expected = "0x2f8788117e7eff1d82e926ec794901d17c78024a50270940304540a733656f0d";
        assert_eq!(format!("{:?}", topic0), expected);
    }

    #[test]
    fn test_role_to_string() {
        let zero_role = B256::ZERO;
        assert_eq!(
            AnyAccessControlHandler::role_to_string(&zero_role),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn test_get_role_name_default() {
        let handler =
            AnyAccessControlHandler::new("accessControl".to_string(), HashMap::new(), None, false);

        assert_eq!(handler.get_role_name(&B256::ZERO), "DEFAULT_ADMIN_ROLE");
    }

    #[test]
    fn test_get_role_name_custom() {
        let role_hash = B256::from([0x9f; 32]);
        let role_str = format!("0x{}", hex::encode(role_hash.as_slice()));

        let role_names = HashMap::from([(role_str.clone(), "CUSTOM_ROLE".to_string())]);

        let handler =
            AnyAccessControlHandler::new("accessControl".to_string(), role_names, None, false);

        assert_eq!(handler.get_role_name(&role_hash), "CUSTOM_ROLE");
    }

    #[test]
    fn test_topic_to_address() {
        // Address is padded to 32 bytes in topics (12 zero bytes + 20 address bytes)
        let mut topic_bytes = [0u8; 32];
        topic_bytes[12..32].copy_from_slice(&[0x42; 20]);
        let topic = B256::from(topic_bytes);

        let address = AnyAccessControlHandler::topic_to_address(&topic);
        assert!(address.is_ok());
        assert_eq!(address.unwrap(), Address::from([0x42; 20]));
    }

    #[test]
    fn test_wrong_handler_definition() {
        let definition = HandlerDefinition::Storage {
            slot: Some(serde_json::json!(5)),
            offset: None,
            return_type: None,
            ignore_relative: None,
        };

        let result =
            AnyAccessControlHandler::from_handler_definition("test".to_string(), definition);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Expected AccessControl")
        );
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test test_access_control_integration --ignored -- --nocapture
    async fn test_access_control_integration() {
        use alloy_provider::RootProvider;
        use std::str::FromStr;

        // Real contract: 0x6a88E8f6B5382d87F39213eB3df43c5FF2498Dd4
        let contract_address =
            Address::from_str("0x6a88E8f6B5382d87F39213eB3df43c5FF2498Dd4").expect("Valid address");

        let rpc_url = "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362"
            .parse()
            .expect("Valid RPC URL");
        let provider = RootProvider::<alloy_provider::network::Ethereum>::new_http(rpc_url);

        let definition = HandlerDefinition::AccessControl {
            role_names: Some(HashMap::from([
                (
                    "0xdf8b4c520ffe197c5343c6f5aec59570151ef9a492f2c624fd45ddde6135ec42"
                        .to_string(),
                    "ADMIN".to_string(),
                ),
                (
                    "0x352d05fe3946dbe49277552ba941e744d5a96d9c60bc1ba0ea5f1d3ae000f7c8"
                        .to_string(),
                    "ORACLE".to_string(),
                ),
                (
                    "0xa615a8afb6fffcb8c6809ac0997b5c9c12b8cc97651150f14c8f6203168cff4c"
                        .to_string(),
                    "UPGRADER".to_string(),
                ),
                (
                    "0xa1496c3abf9cd93b84db10ae569b57fafa04deeeb7ece4167616ad50e35bc56e"
                        .to_string(),
                    "FEE_ADMIN".to_string(),
                ),
            ])),
            pick_role_members: None,
            ignore_relative: Some(false),
            extra: None,
        };

        let handler =
            AccessControlHandler::<alloy_provider::network::Ethereum>::from_handler_definition(
                "accessControl".to_string(),
                definition,
            )
            .expect("Failed to create handler");

        println!("\n=== Testing AccessControlHandler ===");
        println!("Contract: {}", contract_address);

        let result = handler
            .execute(&provider, &contract_address, &HashMap::new())
            .await;

        println!("Field: {}", result.field);
        println!("Error: {:?}", result.error);

        assert!(
            result.error.is_none(),
            "Handler execution failed: {:?}",
            result.error
        );
        assert!(result.value.is_some(), "Expected a value from handler");

        if let Some(HandlerValue::Object(roles)) = result.value {
            println!("Roles found: {}", roles.len());
            for (role_name, _) in roles.iter() {
                println!("  - {}", role_name);
            }
            assert!(roles.contains_key("ADMIN"), "Should have ADMIN role");
        }

        println!("✓ Test passed");
    }
}
