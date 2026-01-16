use alloy_primitives::{Address, B256, keccak256};
use alloy_provider::{Provider, RootProvider, network::Network};
use alloy_rpc_types::Log;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::{self, Value};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::str::FromStr;

#[derive(Clone, Copy)]
enum OperationKind {
    Add,
    Remove,
    Set,
}

impl OperationKind {
    fn label(&self) -> &'static str {
        match self {
            OperationKind::Add => "add",
            OperationKind::Remove => "remove",
            OperationKind::Set => "set",
        }
    }
}

struct OperationContext<'a> {
    kind: OperationKind,
    operation: &'a EventOperation,
    topics0: Vec<(B256, String)>,
}

impl<'a> OperationContext<'a> {
    fn matches(&self, topic: &B256) -> bool {
        self.topics0.iter().any(|(hash, _)| hash == topic)
    }

    fn topics(&self) -> &[(B256, String)] {
        &self.topics0
    }
}

use super::config::{EventOperation, HandlerDefinition};
use super::types::{Handler, HandlerResult, HandlerValue};
use super::utils::{
    EventParameter, canonicalize_event_signature, encode_topic_value, parameter_section,
    value_to_string, values_equal,
};

/// EventHandler fetches and processes historical events from a contract
///
/// Supports three modes:
/// 1. `set` - Returns the most recent matching event
/// 2. `add`/`remove` - Tracks a set of items by processing add/remove events
/// 3. Direct fetch - Returns all matching events
pub struct EventHandler<N: Network> {
    field: String,
    event_signatures: Vec<String>, // Event signatures to match, could be more than one from add/remove/set
    select_fields: Vec<String>,    // Fields to extract from events
    add_operation: Option<EventOperation>, // Add event configuration
    remove_operation: Option<EventOperation>, // Remove event configuration
    set_operation: Option<EventOperation>, // Set event configuration
    group_by: Option<String>,      // GroupBy field name
    dependencies: Vec<String>,
    hidden: bool,
    range: Option<(u64, u64)>, // Optional block range for querying
    indexed: HashMap<String, (String, usize)>, // Field -> (type, topic index), can be in different Event
    _phantom: PhantomData<N>,
}

impl<N: Network> std::fmt::Debug for EventHandler<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHandler")
            .field("field", &self.field)
            .field("event_signatures", &self.event_signatures)
            .field("select_fields", &self.select_fields)
            .field("add_operation", &self.add_operation)
            .field("remove_operation", &self.remove_operation)
            .field("set_operation", &self.set_operation)
            .field("group_by", &self.group_by)
            .field("dependencies", &self.dependencies)
            .field("hidden", &self.hidden)
            .field("range", &self.range)
            .field("indexed", &self.indexed)
            .finish()
    }
}

impl<N: Network> EventHandler<N> {
    /// Create an EventHandler from a handler definition
    pub fn from_handler_definition(field: String, definition: HandlerDefinition) -> Result<Self> {
        match definition {
            HandlerDefinition::Event {
                event,
                select,
                add,
                remove,
                set,
                group_by,
                ignore_relative,
                ..
            } => {
                // Sanitize where clauses in operations
                let add = add.map(EventOperation::sanitize);
                let remove = remove.map(EventOperation::sanitize);
                let set = set.map(EventOperation::sanitize);

                // Parse event signatures
                let event_signatures = if let Some(event_str) = event {
                    vec![event_str]
                } else if add.is_some() || remove.is_some() || set.is_some() {
                    // For add/remove/set mode, we don't need a top-level event field
                    // The event signatures are in the operations
                    vec![]
                } else {
                    return Err(anyhow!(
                        "Event handler requires 'event' field or add/remove/set operations"
                    ));
                };

                // Parse select fields
                let select_fields = if let Some(select_val) = select {
                    match select_val {
                        serde_json::Value::String(s) => vec![s],
                        serde_json::Value::Array(arr) => arr
                            .into_iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect(),
                        _ => vec![],
                    }
                } else {
                    vec![]
                };

                let mut indexed = HashMap::new();
                for event_sig in &event_signatures {
                    indexed.extend(Self::parse_event_index(event_sig)?);
                }
                if let Some(add_op) = add.as_ref() {
                    for event_sig in add_op.events() {
                        indexed.extend(Self::parse_event_index(event_sig)?);
                    }
                }
                if let Some(remove_op) = remove.as_ref() {
                    for event_sig in remove_op.events() {
                        indexed.extend(Self::parse_event_index(event_sig)?);
                    }
                }
                if let Some(set_op) = set.as_ref() {
                    for event_sig in set_op.events() {
                        indexed.extend(Self::parse_event_index(event_sig)?);
                    }
                }

                Ok(Self {
                    field,
                    event_signatures,
                    select_fields,
                    add_operation: add,
                    remove_operation: remove,
                    set_operation: set,
                    group_by,
                    dependencies: vec![],
                    hidden: ignore_relative.unwrap_or(false),
                    range: None,
                    indexed,
                    _phantom: PhantomData,
                })
            }
            HandlerDefinition::AccessControl {
                role_names,
                pick_role_members,
                ignore_relative,
                ..
            } => {
                // OpenZeppelin AccessControl events with proper parameter names
                const ROLE_GRANTED: &str = "RoleGranted(bytes32 indexed role,address indexed account,address indexed sender)";
                const ROLE_REVOKED: &str = "RoleRevoked(bytes32 indexed role,address indexed account,address indexed sender)";
                const DEFAULT_ADMIN_HASH: &str =
                    "0x0000000000000000000000000000000000000000000000000000000000000000";

                let where_clause = if let Some(role_name) = pick_role_members.clone() {
                    if role_name == "DEFAULT_ADMIN_ROLE" {
                        Some(serde_json::json!(["=", "role", DEFAULT_ADMIN_HASH]))
                    } else {
                        let role_hash = if let Some(role_names) = role_names {
                            role_names
                                .get(&role_name)
                                .map(|hash| {
                                    if hash.contains("keccek256") {
                                        keccak256(role_name.clone()).to_string()
                                    } else {
                                        hash.clone()
                                    }
                                })
                                .unwrap_or(keccak256(role_name).to_string())
                        } else {
                            keccak256(role_name).to_string()
                        };
                        Some(serde_json::json!(["=", "role", role_hash]))
                    }
                } else {
                    Some(serde_json::json!(["=", "role", DEFAULT_ADMIN_HASH]))
                };

                let mut indexed = HashMap::new();
                indexed.extend(Self::parse_event_index(ROLE_GRANTED)?);
                indexed.extend(Self::parse_event_index(ROLE_REVOKED)?);

                Ok(Self {
                    field,
                    event_signatures: vec![], // Not used in add/remove mode
                    select_fields: if pick_role_members.is_some() {
                        // When picking specific role, just return account addresses
                        vec!["account".to_string()]
                    } else {
                        // Return both role and account for full info
                        vec!["role".to_string(), "account".to_string()]
                    },
                    add_operation: Some(
                        EventOperation {
                            event: vec![ROLE_GRANTED.to_string()],
                            where_clause: where_clause.clone(),
                        }
                        .sanitize(),
                    ),
                    remove_operation: Some(
                        EventOperation {
                            event: vec![ROLE_REVOKED.to_string()],
                            where_clause,
                        }
                        .sanitize(),
                    ),
                    set_operation: None,
                    group_by: None,
                    dependencies: vec![],
                    hidden: ignore_relative.unwrap_or(false),
                    range: None,
                    indexed,
                    _phantom: PhantomData,
                })
            }
            _ => Err(anyhow!(
                "Expected Event or AccessControl handler definition"
            )),
        }
    }

    /// Parse event signature to extract indexed parameter information
    /// Returns: HashMap<field_name, (type, topic_index)>
    /// Example: "Transfer(address indexed from, address indexed to, uint256 value)"
    /// Returns: {"from": ("address", 1), "to": ("address", 2)}
    fn parse_event_index(event_signature: &str) -> Result<HashMap<String, (String, usize)>> {
        let mut indexed_params = HashMap::new();

        if let Some(params_str) = parameter_section(event_signature) {
            if params_str.trim().is_empty() {
                return Ok(indexed_params);
            }

            let mut topic_index = 1; // topic[0] is event signature, indexed params start at topic[1]

            for param in params_str.split(',') {
                if let Some(EventParameter { typ, name, indexed }) =
                    EventParameter::parse(param.trim())
                    && indexed
                {
                    let field_name = name.unwrap_or_else(|| format!("topic{}", topic_index));
                    indexed_params.insert(field_name, (typ, topic_index));
                    topic_index += 1;
                }
            }
        }

        Ok(indexed_params)
    }

    /// Compute topic hash for a specific topic index
    /// topic_index 0: Event signature hash
    /// topic_index 1-3: Indexed parameter values
    #[allow(dead_code)]
    fn compute_topic(&self, topic_index: usize, value: Option<Value>) -> Option<B256> {
        match topic_index {
            0 => {
                // Return event signature hash for the first event signature
                // In add/remove mode, this might not be meaningful
                self.event_signatures.first().map(|sig| {
                    alloy_primitives::keccak256(canonicalize_event_signature(sig).as_bytes())
                })
            }
            1..=3 => {
                // For indexed parameters, encode the provided value
                value.as_ref().and_then(encode_topic_value)
            }
            _ => None,
        }
    }

    /// Compute topic0 (event signature hash) from event signature string
    fn compute_topic0(event_sig: &str) -> B256 {
        let canonical_sig = canonicalize_event_signature(event_sig);
        alloy_primitives::keccak256(canonical_sig.as_bytes())
    }

    /// Fetch logs for a specific topic0 in batches
    /// RPC endpoints may limit queries (Ankr limits to ~1000 blocks)
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

    /// Parse a log into a HashMap of field values using indexed parameter information
    fn parse_log(&self, log: &Log) -> Result<HashMap<String, HandlerValue>> {
        let mut result = HashMap::new();
        let topics = log.topics();

        // Parse indexed parameters using our indexed parameter map
        for (field_name, (param_type, topic_idx)) in &self.indexed {
            if let Some(topic) = topics.get(*topic_idx) {
                let value = Self::parse_topic_value(topic, param_type)?;
                result.insert(field_name.clone(), value);
            }
        }

        // Store raw data for non-indexed parameters (would need ABI for proper parsing)
        let log_data = log.data();
        if !log_data.data.is_empty() {
            result.insert(
                "data".to_string(),
                HandlerValue::Bytes(log_data.data.clone()),
            );
        }

        Ok(result)
    }

    /// Parse a topic value based on parameter type
    fn parse_topic_value(topic: &B256, param_type: &str) -> Result<HandlerValue> {
        match param_type {
            "bool" => {
                // Boolean: all zeros except last byte is 0 or 1
                let is_boolean =
                    topic[0..31].iter().all(|&b| b == 0) && (topic[31] == 0 || topic[31] == 1);

                if is_boolean {
                    Ok(HandlerValue::Boolean(topic[31] == 1))
                } else {
                    Err(anyhow!("Invalid boolean value in topic"))
                }
            }
            "address" => {
                // Address: last 20 bytes, first 12 should be zero
                if topic[0..12].iter().all(|&b| b == 0) {
                    let addr_bytes = &topic[12..32];
                    if let Ok(addr) = Address::try_from(addr_bytes) {
                        Ok(HandlerValue::Address(addr))
                    } else {
                        Err(anyhow!("Invalid address in topic"))
                    }
                } else {
                    // Store as bytes if not properly padded
                    Ok(HandlerValue::Bytes(topic.0.into()))
                }
            }
            t if t.starts_with("uint") || t.starts_with("int") => {
                // Store as bytes for now - proper uint256 parsing would need big integer library
                Ok(HandlerValue::Bytes(topic.0.into()))
            }
            "bytes32" => Ok(HandlerValue::Bytes(topic.0.into())),
            _ => {
                // Default: store as bytes
                Ok(HandlerValue::Bytes(topic.0.into()))
            }
        }
    }

    /// Extract selected fields from parsed event data
    fn extract_fields(&self, parsed: &HashMap<String, HandlerValue>) -> HandlerValue {
        if self.select_fields.is_empty() {
            // Return all fields as object
            HandlerValue::Object(parsed.clone())
        } else if self.select_fields.len() == 1 {
            // Return single field directly
            parsed
                .get(&self.select_fields[0])
                .cloned()
                .unwrap_or(HandlerValue::String("null".to_string()))
        } else {
            // Return selected fields as object
            let mut selected = HashMap::new();
            for field in &self.select_fields {
                if let Some(value) = parsed.get(field) {
                    selected.insert(field.clone(), value.clone());
                }
            }
            HandlerValue::Object(selected)
        }
    }

    /// Execute event processing with custom block range - supports add/remove/set modes and groupBy
    async fn execute_range_inner(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        from_block: u64,
        to_block: u64,
    ) -> Result<HandlerValue> {
        let add_op = self.add_operation.as_ref();
        let remove_op = self.remove_operation.as_ref();
        let mut set_op = self.set_operation.clone();

        println!(
            "  Block range: {} to {} (range: {})",
            from_block,
            to_block,
            to_block - from_block
        );

        if add_op.is_none() && remove_op.is_none() && set_op.is_none() {
            let event_sig = self.event_signatures.first().unwrap();
            set_op = Some(EventOperation {
                event: vec![event_sig.clone()],
                where_clause: None,
            });
        }

        let add_ctx = add_op.map(|op| OperationContext {
            kind: OperationKind::Add,
            operation: op,
            topics0: op
                .events()
                .iter()
                .map(|event_sig| (Self::compute_topic0(event_sig), event_sig.clone()))
                .collect(),
        });
        let remove_ctx = remove_op.map(|op| OperationContext {
            kind: OperationKind::Remove,
            operation: op,
            topics0: op
                .events()
                .iter()
                .map(|event_sig| (Self::compute_topic0(event_sig), event_sig.clone()))
                .collect(),
        });
        let set_ctx = set_op.as_ref().map(|op| OperationContext {
            kind: OperationKind::Set,
            operation: op,
            topics0: op
                .events()
                .iter()
                .map(|event_sig| (Self::compute_topic0(event_sig), event_sig.clone()))
                .collect(),
        });

        // Fetch logs for all operation types
        let mut all_logs = Vec::new();

        if let Some(ctx) = &add_ctx {
            for (topic0, event_sig) in ctx.topics() {
                println!(
                    "  Fetching add events: {} -> topic0: {:?}",
                    event_sig, topic0
                );
                let logs =
                    Self::fetch_logs_batched(provider, address, *topic0, from_block, to_block)
                        .await?;
                println!("  Found {} add events for {}", logs.len(), event_sig);
                all_logs.extend(logs);
            }
        }

        if let Some(ctx) = &remove_ctx {
            for (topic0, event_sig) in ctx.topics() {
                println!(
                    "  Fetching remove events: {} -> topic0: {:?}",
                    event_sig, topic0
                );
                let logs =
                    Self::fetch_logs_batched(provider, address, *topic0, from_block, to_block)
                        .await?;
                println!("  Found {} remove events for {}", logs.len(), event_sig);
                all_logs.extend(logs);
            }
        }

        if let Some(ctx) = &set_ctx {
            for (topic0, event_sig) in ctx.topics() {
                println!(
                    "  Fetching set events: {} -> topic0: {:?}",
                    event_sig, topic0
                );
                let logs =
                    Self::fetch_logs_batched(provider, address, *topic0, from_block, to_block)
                        .await?;
                println!("  Found {} set events for {}", logs.len(), event_sig);
                all_logs.extend(logs);
            }
        }

        // CRITICAL: Sort by block number and log index for chronological processing
        all_logs.sort_by(|a, b| match a.block_number.cmp(&b.block_number) {
            std::cmp::Ordering::Equal => a.log_index.cmp(&b.log_index),
            other => other,
        });

        println!("  Total logs to process: {}", all_logs.len());

        self.process_logs(&all_logs, &add_ctx, &remove_ctx, &set_ctx)
    }

    /// Process logs with unified grouping logic - uses field name as default group when no groupBy specified
    fn process_logs(
        &self,
        all_logs: &[alloy_rpc_types::Log],
        add_ctx: &Option<OperationContext>,
        remove_ctx: &Option<OperationContext>,
        set_ctx: &Option<OperationContext>,
    ) -> Result<HandlerValue> {
        // Use the groupBy field if specified, otherwise use the field name itself as the group
        let group_by_field = self.group_by.as_deref();
        let default_group_name = &self.field;

        let mut grouped_results: HashMap<String, HashMap<String, HandlerValue>> = HashMap::new();

        for log in all_logs {
            let Some(topic0) = log.topics().first() else {
                continue;
            };
            let parsed_log = self.parse_log(log)?;

            // Determine the group key
            let group_key = if let Some(field) = group_by_field {
                // Use the specified groupBy field value
                match parsed_log.get(field) {
                    Some(value) => value_to_string(value),
                    None => continue, // Skip if groupBy field not found
                }
            } else {
                // Use the field name itself as the group
                default_group_name.clone()
            };

            if let Some(ctx) = add_ctx.as_ref().filter(|ctx| ctx.matches(topic0)) {
                if self.should_apply_operation(ctx, &parsed_log) {
                    let value = self.extract_fields(&parsed_log);
                    let item_key = value_to_string(&value);

                    grouped_results
                        .entry(group_key)
                        .or_default()
                        .insert(item_key, value);
                }
                continue;
            }

            if let Some(ctx) = remove_ctx.as_ref().filter(|ctx| ctx.matches(topic0)) {
                if self.should_apply_operation(ctx, &parsed_log) {
                    let value = self.extract_fields(&parsed_log);
                    let item_key = value_to_string(&value);

                    if let Some(group) = grouped_results.get_mut(&group_key) {
                        group.remove(&item_key);
                    }
                }
                continue;
            }

            if let Some(ctx) = set_ctx.as_ref().filter(|ctx| ctx.matches(topic0)) {
                if self.should_apply_operation(ctx, &parsed_log) {
                    let value = self.extract_fields(&parsed_log);

                    // For set operations, replace the entire group with the new value
                    let mut new_group = HashMap::new();
                    new_group.insert(group_key.clone(), value);
                    grouped_results.insert(group_key, new_group);
                }
                continue;
            }
        }

        // Convert grouped results to final format
        if group_by_field.is_some() {
            // When groupBy is specified, return object with multiple groups
            let mut final_result = HashMap::new();
            for (group_key, group_items) in grouped_results {
                let mut items: Vec<HandlerValue> = group_items.into_values().collect();
                items.sort_by_key(value_to_string);
                final_result.insert(group_key, HandlerValue::Array(items));
            }

            println!("  Final result: {} groups", final_result.len());
            Ok(HandlerValue::Object(final_result))
        } else {
            // When no groupBy, return array from the single default group
            let items = grouped_results
                .get(default_group_name)
                .map(|group_items| {
                    let mut items: Vec<HandlerValue> = group_items.values().cloned().collect();
                    items.sort_by_key(value_to_string);
                    items
                })
                .unwrap_or_default();

            println!("  Final result: {} tracked items", items.len());
            Ok(HandlerValue::Array(items))
        }
    }

    fn should_apply_operation(
        &self,
        context: &OperationContext<'_>,
        parsed: &HashMap<String, HandlerValue>,
    ) -> bool {
        if let Some(where_clause) = &context.operation.where_clause {
            match self.evaluate_where_clause(where_clause, parsed) {
                Ok(result) => result,
                Err(e) => {
                    println!(
                        "Failed to evaluate {} where clause: {}",
                        context.kind.label(),
                        e
                    );
                    false
                }
            }
        } else {
            true
        }
    }

    /// Evaluate a where clause against parsed event data
    /// Where clause format: ["operator", "field", "value"]
    fn evaluate_where_clause(
        &self,
        where_clause: &Value,
        parsed_data: &HashMap<String, HandlerValue>,
    ) -> Result<bool> {
        // If where clause is null/missing, always pass
        if where_clause.is_null() {
            return Ok(true);
        }

        // Parse S-expression: [operator, field, value]
        let arr = where_clause
            .as_array()
            .ok_or_else(|| anyhow!("Where clause must be an array"))?;

        if arr.len() != 3 {
            return Err(anyhow!(
                "Where clause must have exactly 3 elements: [operator, field, value]"
            ));
        }

        let operator = arr[0]
            .as_str()
            .ok_or_else(|| anyhow!("Where clause operator must be a string"))?;

        let field_name = arr[1]
            .as_str()
            .ok_or_else(|| anyhow!("Where clause field must be a string"))?;

        // Remove '#' prefix if present (used to indicate indexed topic fields)
        let field_name = field_name.trim_start_matches('#');

        let expected_value = &arr[2];

        // Get the actual value from parsed data
        let actual_value = parsed_data
            .get(field_name)
            .ok_or_else(|| anyhow!("Field '{}' not found in event data", field_name))?;

        // Evaluate based on operator
        match operator {
            "=" | "==" => Ok(values_equal(actual_value, expected_value)),
            "!=" => Ok(!values_equal(actual_value, expected_value)),
            _ => Err(anyhow!("Unsupported where clause operator: {}", operator)),
        }
    }
    // values_equal, encode_topic_value, and value_to_string are provided by utils module

    /// Execute with custom block range
    pub async fn execute_range(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        from_block: u64,
        to_block: u64,
    ) -> HandlerResult {
        match self
            .execute_range_inner(provider, address, from_block, to_block)
            .await
        {
            Ok(value) => HandlerResult {
                field: self.field.clone(),
                value: Some(value),
                error: None,
                hidden: self.hidden,
            },
            Err(e) => HandlerResult {
                field: self.field.clone(),
                value: None,
                error: Some(format!("Failed to process events: {}", e)),
                hidden: self.hidden,
            },
        }
    }

    /// Execute around the contract's sinceBlock (±5 blocks) from discovered.json
    pub async fn execute_match_discovered(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
        discovered: &crate::discovered::DiscoveredJson,
    ) -> HandlerResult {
        let current_block = match provider.get_block_number().await {
            Ok(block) => block,
            Err(e) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to get current block: {}", e)),
                    hidden: self.hidden,
                };
            }
        };

        // Try to find the entry for the provided address and use its sinceBlock
        let target_since_block = discovered.entries.iter().find_map(|entry| {
            let raw_address = entry
                .address
                .strip_prefix("eth:")
                .unwrap_or(entry.address.as_str());
            Address::from_str(raw_address)
                .ok()
                .filter(|entry_address| entry_address == address)
                .and(entry.since_block)
        });

        let window = 3;
        let (from_block, to_block) = if let Some(since_block) = target_since_block {
            (
                since_block.saturating_sub(window),
                since_block.saturating_add(window).min(current_block),
            )
        } else {
            let fallback_from = discovered
                .entries
                .iter()
                .filter_map(|entry| entry.since_block)
                .min()
                .unwrap_or(current_block.saturating_sub(window));
            (fallback_from, current_block)
        };

        self.execute_range(provider, address, from_block, to_block)
            .await
    }
}

#[async_trait]
impl<N: alloy_provider::network::Network> Handler<N> for EventHandler<N> {
    fn field(&self) -> &str {
        &self.field
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
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
        // Get current block number for range
        let to_block = match provider.get_block_number().await {
            Ok(block) => block,
            Err(e) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to get current block: {}", e)),
                    hidden: self.hidden,
                };
            }
        };

        // Use configured range or default to last 10 blocks
        let (from_block, to_block) = if let Some((from, to)) = self.range {
            (from, to)
        } else {
            (to_block.saturating_sub(5), to_block)
        };

        self.execute_range(provider, address, from_block, to_block)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::config::{
        ContractConfig, DiscoveryConfig, HandlerDefinition, jsonc_to_serde_value,
    };
    use alloy_primitives::Address;
    use alloy_provider::network::AnyNetwork;
    use aomi_anvil::default_provider;
    use serde_json::json;
    use std::{fs, path::Path, str::FromStr};
    use tokio::runtime::Runtime;

    fn load_discovery_config(relative_path: &str) -> Option<DiscoveryConfig> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        if !path.exists() {
            eprintln!("Skipping: missing config file {}", path.display());
            return None;
        }
        let content = fs::read_to_string(&path).expect("read config");
        let parsed = jsonc_parser::parse_to_value(&content, &Default::default())
            .expect("parse jsonc")
            .expect("json value");
        Some(serde_json::from_value(jsonc_to_serde_value(parsed)).expect("deserialize config"))
    }

    fn contract_from_path(
        config_rel_path: &str,
        contract_address: &str,
    ) -> Option<ContractConfig> {
        let config = load_discovery_config(config_rel_path)?;
        let overrides = config.overrides.as_ref().expect("missing overrides");
        Some(
            overrides
                .get(contract_address)
                .unwrap_or_else(|| panic!("contract {} missing", contract_address))
                .clone(),
        )
    }

    // fn event_handler_from_definition(
    //     field: &str,
    //     definition: HandlerDefinition,
    // ) -> EventHandler<AnyNetwork> {
    //     EventHandler::from_handler_definition(field.to_string(), definition)
    //         .expect("failed to build event handler")
    // }

    fn should_execute_tests() -> bool {
        matches!(
            std::env::var("EXECUTE").as_deref(),
            Ok("true" | "TRUE" | "1")
        )
    }

    fn maybe_execute_handler(
        handler: &EventHandler<AnyNetwork>,
        discovered: Option<&crate::discovered::DiscoveredJson>,
        contract_address: &str,
        field_name: &str,
    ) {
        if !should_execute_tests() {
            return;
        }

        let address = Address::from_str(contract_address)
            .unwrap_or_else(|e| panic!("Invalid address {}: {}", contract_address, e));

        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        let provider = runtime
            .block_on(default_provider())
            .expect("Set providers.toml when running with EXECUTE=true");

        if let Some(discovered) = discovered {
            let result =
                runtime.block_on(handler.execute_match_discovered(&provider, &address, discovered));

            println!(
                "[EXECUTE] {}::{} [discovered] => value: {:?}, error: {:?}",
                contract_address, field_name, result.value, result.error
            );
        } else {
            let to_block = runtime
                .block_on(provider.get_block_number())
                .expect("Failed to fetch current block number");
            let from_block = to_block.saturating_sub(9);

            let result =
                runtime.block_on(handler.execute_range(&provider, &address, from_block, to_block));

            println!(
                "[EXECUTE] {}::{} [blocks {}-{}] => value: {:?}, error: {:?}",
                contract_address, field_name, from_block, to_block, result.value, result.error
            );
        }
    }

    #[test]
    fn test_canonicalize() {
        // Test with parameter names and indexed keywords
        assert_eq!(
            canonicalize_event_signature(
                "Transfer(address indexed from, address to, uint256 value)"
            ),
            "Transfer(address,address,uint256)"
        );

        // Test with only indexed keywords
        assert_eq!(
            canonicalize_event_signature(
                "RoleGranted(bytes32 indexed role, address indexed account)"
            ),
            "RoleGranted(bytes32,address)"
        );

        // Test already canonical
        assert_eq!(
            canonicalize_event_signature("Transfer(address,address,uint256)"),
            "Transfer(address,address,uint256)"
        );

        // Test empty params
        assert_eq!(canonicalize_event_signature("NoParams()"), "NoParams()");
    }

    #[test]
    fn test_parse_event_index() {
        let definition = HandlerDefinition::Event {
            event: Some(
                "Transfer(address indexed from, address indexed to, uint256 value)".to_string(),
            ),
            return_type: None,
            select: None,
            add: None,
            remove: None,
            set: None,
            group_by: None,
            ignore_relative: None,
        };

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition("test".to_string(), definition)
                .unwrap();

        let indexed = EventHandler::<AnyNetwork>::parse_event_index(
            "Transfer(address indexed from, address indexed to, uint256 value)",
        )
        .unwrap();

        assert_eq!(indexed.len(), 2);
        assert_eq!(indexed.get("from"), Some(&("address".to_string(), 1)));
        assert_eq!(indexed.get("to"), Some(&("address".to_string(), 2)));
        // "value" is not indexed, so it shouldn't be in the map
        assert!(!indexed.contains_key("value"));
        assert_eq!(handler.indexed.len(), 2);
    }

    // #[test]
    // fn test_parse_from_json() {
    //     // use alloy::primitives::{Address, Log, LogData, U256};
    //     // use alloy::rpc::types::eth::Filter;
    //     // use std::str::FromStr;

    //     // Parse the JSON configuration
    //     let json_config = r#"{
    //       "type": "event",
    //       "select": ["spokePool", "adapter", "messageService"],
    //       "groupBy": "l2ChainId",
    //       "ignoreRelative": false,
    //       "add": {
    //           "event": ["CrossChainContractsSet", "CrossChainContractsUpdated"],
    //           "where": ["and",
    //             ["!=", "#spokePool", "0x0000000000000000000000000000000000000000"],
    //             ["or",
    //               ["=", "#enabled", true],
    //               [">=", "#timestamp", 1672531200]
    //             ],
    //             ["not", ["=", "#deprecated", true]]
    //           ]
    //         },
    //       "remove": {
    //           "event": ["CrossChainContractsRemoved", "ContractDeprecated"],
    //           "where": ["or",
    //             ["=", "#removed", true],
    //             ["and",
    //               ["<", "#lastActivity", 1640995200],
    //               ["!=", "#protected", true]
    //             ]
    //           ]
    //         }
    //     }"#;

    //     let json_config_single = r#"{
    //       "type": "event",
    //       "select": ["spokePool", "adapter", "messageService"],
    //       "groupBy": "l2ChainId",
    //       "ignoreRelative": false,
    //       "add": {
    //           "event": "CrossChainContractsSet",
    //           "where": ["and",
    //             ["!=", "#spokePool", "0x0000000000000000000000000000000000000000"],
    //             ["or",
    //               ["=", "#enabled", true],
    //               [">=", "#timestamp", 1672531200]
    //             ],
    //             ["not", ["=", "#deprecated", true]]
    //           ]
    //         },
    //       "remove": {
    //           "event": "CrossChainContractsRemoved",
    //           "where": ["or",
    //             ["=", "#removed", true],
    //             ["and",
    //               ["<", "#lastActivity", 1640995200],
    //               ["!=", "#protected", true]
    //             ]
    //           ]
    //         }
    //     }"#;

    //     let parsed_multi: serde_json::Value = serde_json::from_str(json_config).unwrap();
    //     let handler_def_multi: HandlerDefinition = serde_json::from_value(parsed_multi).unwrap();
    //     let handler_multi =
    //         event_handler_from_definition("CrossChainBridgeState", handler_def_multi);

    //     assert_eq!(handler_multi.field, "CrossChainBridgeState");
    //     assert_eq!(
    //         handler_multi.select_fields,
    //         vec!["spokePool", "adapter", "messageService"]
    //     );
    //     assert_eq!(handler_multi.group_by, Some("l2ChainId".to_string()));

    //     let add_multi = handler_multi
    //         .add_operation
    //         .as_ref()
    //         .expect("multi add missing");
    //     assert_eq!(
    //         add_multi.events(),
    //         &[
    //             "CrossChainContractsSet".to_string(),
    //             "CrossChainContractsUpdated".to_string()
    //         ]
    //     );
    //     let add_multi_where = add_multi.where_clause.as_ref().expect("multi add where");
    //     assert!(
    //         !add_multi_where.to_string().contains('#'),
    //         "expected sanitized add where clause without '#'"
    //     );

    //     let remove_multi = handler_multi
    //         .remove_operation
    //         .as_ref()
    //         .expect("multi remove missing");
    //     assert_eq!(
    //         remove_multi.events(),
    //         &[
    //             "CrossChainContractsRemoved".to_string(),
    //             "ContractDeprecated".to_string()
    //         ]
    //     );
    //     let remove_multi_where = remove_multi
    //         .where_clause
    //         .as_ref()
    //         .expect("multi remove where");
    //     assert!(
    //         !remove_multi_where.to_string().contains('#'),
    //         "expected sanitized remove where clause without '#'"
    //     );

    //     let parsed_single: serde_json::Value = serde_json::from_str(json_config_single).unwrap();
    //     let handler_def: HandlerDefinition = serde_json::from_value(parsed_single).unwrap();
    //     let handler = event_handler_from_definition("CrossChainBridgeState", handler_def);

    //     // Verify handler configuration
    //     assert_eq!(handler.field, "CrossChainBridgeState");
    //     assert_eq!(
    //         handler.select_fields,
    //         vec!["spokePool", "adapter", "messageService"]
    //     );
    //     assert_eq!(handler.group_by, Some("l2ChainId".to_string()));
    //     assert!(handler.add_operation.is_some());
    //     assert!(handler.remove_operation.is_some());
    //     assert!(handler.set_operation.is_none());

    //     let add_single = handler.add_operation.as_ref().unwrap();
    //     assert_eq!(add_single.events(), &["CrossChainContractsSet".to_string()]);
    //     let add_single_where = add_single.where_clause.as_ref().unwrap();
    //     assert!(
    //         !add_single_where.to_string().contains('#'),
    //         "expected sanitized single add where clause without '#'"
    //     );

    //     let remove_single = handler.remove_operation.as_ref().unwrap();
    //     assert_eq!(
    //         remove_single.events(),
    //         &["CrossChainContractsRemoved".to_string()]
    //     );
    //     let remove_single_where = remove_single.where_clause.as_ref().unwrap();
    //     assert!(
    //         !remove_single_where.to_string().contains('#'),
    //         "expected sanitized single remove where clause without '#'"
    //     );

    //     // Create fake logs that would match our events
    //     let contract_address =
    //         Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

    //     // // Log 1: CrossChainContractsSet for l2ChainId=1
    //     // let log1 = Log {
    //     //     address: contract_address,
    //     //     data: LogData::new_unchecked(
    //     //         vec![
    //     //             EventHandler::<AnyNetwork>::compute_topic0("CrossChainContractsSet(uint256,address,address,address,bool,bool)"),
    //     //             U256::from(1).into(), // l2ChainId = 1 (indexed)
    //     //         ],
    //     //         // ABI encoded data: spokePool, adapter, messageService, enabled, deprecated
    //     //         hex::decode("000000000000000000000000abcdef1234567890123456789012345678901234000000000000000000000000def1234567890123456789012345678901234560000000000000000000000001111111111111111111111111111111111111111000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
    //     //     ),
    //     // };

    //     // // Log 2: CrossChainContractsSet for l2ChainId=1 (second contract)
    //     // let log2 = Log {
    //     //     address: contract_address,
    //     //     data: LogData::new_unchecked(
    //     //         vec![
    //     //             EventHandler::<AnyNetwork>::compute_topic0("CrossChainContractsSet(uint256,address,address,address,bool,bool)"),
    //     //             U256::from(1).into(), // l2ChainId = 1 (indexed)
    //     //         ],
    //     //         // ABI encoded data: spokePool, adapter, messageService, enabled, deprecated
    //     //         hex::decode("000000000000000000000000222222222222222222222222222222222222222200000000000000000000000033333333333333333333333333333333333333330000000000000000000000004444444444444444444444444444444444444444000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
    //     //     ),
    //     // };

    //     // // Log 3: CrossChainContractsSet for l2ChainId=10
    //     // let log3 = Log {
    //     //     address: contract_address,
    //     //     data: LogData::new_unchecked(
    //     //         vec![
    //     //             EventHandler::<AnyNetwork>::compute_topic0("CrossChainContractsSet(uint256,address,address,address,bool,bool)"),
    //     //             U256::from(10).into(), // l2ChainId = 10 (indexed)
    //     //         ],
    //     //         hex::decode("000000000000000000000000555555555555555555555555555555555555555500000000000000000000000066666666666666666666666666666666666666660000000000000000000000007777777777777777777777777777777777777777000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
    //     //     ),
    //     // };

    //     // // Log 4: CrossChainContractsSet for l2ChainId=42161 (Arbitrum)
    //     // let log4 = Log {
    //     //     address: contract_address,
    //     //     data: LogData::new_unchecked(
    //     //         vec![
    //     //             EventHandler::<AnyNetwork>::compute_topic0("CrossChainContractsSet(uint256,address,address,address,bool,bool)"),
    //     //             U256::from(42161).into(), // l2ChainId = 42161 (indexed)
    //     //         ],
    //     //         hex::decode("000000000000000000000000888888888888888888888888888888888888888800000000000000000000000099999999999999999999999999999999999999990000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000").unwrap().into(),
    //     //     ),
    //     // };

    //     // let logs = vec![log1, log2, log3, log4];

    //     // // Process the logs manually to simulate handler execution
    //     // // This would normally be done by the execute_range method
    //     // let result = handler.process_logs(&logs, Some("l2ChainId"));

    //     // // Verify the expected grouped structure
    //     // let expected_json = serde_json::json!({
    //     //     "1": [
    //     //         {"spokePool": "0xabcdef1234567890123456789012345678901234", "adapter": "0xdef1234567890123456789012345678901234567", "messageService": "0x1111111111111111111111111111111111111111"},
    //     //         {"spokePool": "0x2222222222222222222222222222222222222222", "adapter": "0x3333333333333333333333333333333333333333", "messageService": "0x4444444444444444444444444444444444444444"}
    //     //     ],
    //     //     "10": [
    //     //         {"spokePool": "0x5555555555555555555555555555555555555555", "adapter": "0x6666666666666666666666666666666666666666", "messageService": "0x7777777777777777777777777777777777777777"}
    //     //     ],
    //     //     "42161": [
    //     //         {"spokePool": "0x8888888888888888888888888888888888888888", "adapter": "0x9999999999999999999999999999999999999999", "messageService": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
    //     //     ]
    //     // });

    //     // println!("Test completed - handler configuration parsed and logs would be processed correctly");
    //     // println!("Expected result structure: {}", expected_json);
    // }

    #[test]
    fn test_compute_topic0() {
        let topic0 =
            EventHandler::<AnyNetwork>::compute_topic0("Transfer(address,address,uint256)");

        // ERC20 Transfer event signature
        let expected = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
        assert_eq!(format!("{:?}", topic0), expected);
    }

    #[test]
    fn test_compute_topic_with_value() {
        let definition = HandlerDefinition::Event {
            event: Some(
                "Transfer(address indexed from, address indexed to, uint256 value)".to_string(),
            ),
            return_type: None,
            select: None,
            add: None,
            remove: None,
            set: None,
            group_by: None,
            ignore_relative: None,
        };

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition("test".to_string(), definition)
                .unwrap();

        // Test topic0 (event signature)
        let topic0 = handler.compute_topic(0, None);
        assert!(topic0.is_some());

        // Test topic1 with address value
        let address_value = serde_json::json!("0x742d35Cc6634C0532925a3b8D9Db4B9b7203d6fD");
        let topic1 = handler.compute_topic(1, Some(address_value));
        assert!(topic1.is_some());

        // Test topic2 with boolean value
        let bool_value = serde_json::json!(true);
        let topic2 = handler.compute_topic(2, Some(bool_value));
        assert!(topic2.is_some());

        // Test invalid topic index
        let invalid_topic = handler.compute_topic(4, None);
        assert!(invalid_topic.is_none());
    }

    #[test]
    fn test_deserialize_shared_eigenlayer_minters_event_handler() {
        let config_path = "../data/shared-eigenlayer/ethereum/config.jsonc";
        let contract_address = "0x83E9115d334D248Ce39a6f36144aEaB5b3456e75";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };
        let field_name = "Minters";
        let HandlerDefinition::Event {
            event,
            select,
            add,
            remove,
            set,
            group_by,
            ..
        } = contract
            .fields
            .clone()
            .unwrap()
            .get(field_name)
            .cloned()
            .unwrap()
            .handler
            .unwrap()
        else {
            panic!("unexpected handler variant");
        };

        assert!(event.is_none());
        assert_eq!(
            select.as_ref().and_then(|v| v.as_str()),
            Some("minterAddress")
        );

        let add = add.as_ref().expect("add operation missing");
        assert_eq!(add.events(), &["IsMinterModified".to_string()]);
        assert_eq!(
            add.where_clause.as_ref(),
            Some(&json!(["=", "#newStatus", true]))
        );

        let remove = remove.as_ref().expect("remove operation missing");
        assert_eq!(remove.events(), &["IsMinterModified".to_string()]);
        assert_eq!(
            remove.where_clause.as_ref(),
            Some(&json!(["!=", "#newStatus", true]))
        );

        // Verify new fields are None for this test case
        assert!(set.is_none());
        assert!(group_by.is_none());
    }

    #[test]
    fn test_deserialize_cbridge_sentinel_events() {
        let config_path = "../data/cbridge/ethereum/config.jsonc";
        let contract_address = "0xF140024969F6c76494a78518D9a99c8776B55f70";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };
        let field_name = "governors";
        let HandlerDefinition::Event {
            add,
            remove,
            select,
            set,
            group_by,
            ..
        } = contract
            .fields
            .clone()
            .unwrap()
            .get(field_name)
            .cloned()
            .unwrap()
            .handler
            .unwrap()
        else {
            panic!("unexpected handler variant");
        };

        assert_eq!(select.as_ref().and_then(|v| v.as_str()), Some("account"));
        let add = add.as_ref().expect("add missing");
        assert_eq!(
            add.where_clause.as_ref(),
            Some(&json!(["=", "#added", true]))
        );

        let remove = remove.as_ref().expect("remove missing");
        assert_eq!(
            remove.where_clause.as_ref(),
            Some(&json!(["!=", "#added", true]))
        );

        // Verify new fields are None for this test case
        assert!(set.is_none());
        assert!(group_by.is_none());
    }

    #[test]
    fn test_deserialize_morph_challengers_event_handler() {
        let config_path = "../data/morph/ethereum/config.jsonc";
        let contract_address = "0x759894Ced0e6af42c26668076Ffa84d02E3CeF60";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };
        let field_name = "challengers";
        let definition = contract
            .fields
            .clone()
            .unwrap()
            .get(field_name)
            .cloned()
            .unwrap()
            .handler
            .unwrap();
        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition(field_name.to_string(), definition)
                .unwrap();

        assert_eq!(handler.select_fields, ["account"]);
        let add = handler.add_operation.as_ref().expect("add missing");
        assert_eq!(
            add.where_clause.as_ref(),
            Some(&json!(["=", "status", true]))
        );

        let remove = handler.remove_operation.as_ref().expect("remove missing");
        assert_eq!(
            remove.where_clause.as_ref(),
            Some(&json!(["!=", "status", true]))
        );

        // Verify new fields are None for this test case
        assert!(handler.set_operation.is_none());
        assert!(handler.group_by.is_none());
    }

    #[test]
    fn test_deserialize_optimism_deleted_outputs_event_handler() {
        let config_path = "../data/optimism/ethereum/config.jsonc";
        let contract_address = "0xdfe97868233d1aa22e815a266982f2cf17685a27";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };
        let field_name = "deletedOutputs";
        let HandlerDefinition::Event {
            add,
            remove,
            select,
            set,
            group_by,
            ..
        } = contract
            .fields
            .clone()
            .unwrap()
            .get(field_name)
            .cloned()
            .unwrap()
            .handler
            .unwrap()
        else {
            panic!("unexpected handler variant");
        };

        let select = select.as_ref().expect("select missing");
        let values: Vec<&str> = select
            .as_array()
            .expect("select should be array")
            .iter()
            .map(|v| v.as_str().expect("array entry should be string"))
            .collect();
        assert_eq!(values, ["prevNextOutputIndex", "newNextOutputIndex"]);

        let add = add.as_ref().expect("add missing");
        assert_eq!(add.events(), &["OutputsDeleted".to_string()]);
        assert!(add.where_clause.is_none());
        assert!(remove.is_none());

        // Verify new fields are None for this test case
        assert!(set.is_none());
        assert!(group_by.is_none());
    }

    #[test]
    fn test_deserialize_grvt_validators_and_access_control() {
        let config_path = "../data/grvt/ethereum/config.jsonc";
        let contract_address = "0x8c0Bfc04AdA21fd496c55B8C50331f904306F564";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };
        let field_name = "validatorsVTL";

        let definition = contract
            .fields
            .clone()
            .unwrap()
            .get(field_name)
            .cloned()
            .unwrap()
            .handler
            .unwrap();

        let HandlerDefinition::Event {
            select: _select,
            add: _add,
            remove: _remove,
            ..
        } = definition.clone()
        else {
            panic!("unexpected handler variant");
        };

        let config_path = "../data/grvt/ethereum/config.jsonc";
        let contract_address = "0x3Cd52B238Ac856600b22756133eEb31ECb25109a";
        let Some(contract) = contract_from_path(config_path, contract_address) else {
            return;
        };

        let whitelist_definition = contract
            .fields
            .clone()
            .unwrap()
            .get("whitelistedSender")
            .cloned()
            .unwrap()
            .handler
            .unwrap();
        println!("whitelist_definition: {:?}", whitelist_definition);

        let HandlerDefinition::AccessControl {
            pick_role_members, ..
        } = whitelist_definition
        else {
            panic!("unexpected handler variant");
        };
        assert_eq!(pick_role_members.as_deref(), Some("L2_TX_SENDER_ROLE"));

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition(field_name.to_string(), definition)
                .unwrap();

        assert_eq!(handler.select_fields, ["validator"]);

        assert_eq!(
            handler.add_operation.unwrap().where_clause.as_ref(),
            Some(&json!(["=", "chainId", 325]))
        );
        assert_eq!(
            handler.remove_operation.unwrap().where_clause.as_ref(),
            Some(&json!(["=", "chainId", 325]))
        );
    }

    #[test]
    fn test_deserialize_kroma_admin_changed_event_handler() {
        // Create an AccessControl handler definition for admin access control
        let definition = HandlerDefinition::AccessControl {
            role_names: None,
            pick_role_members: None,
            ignore_relative: None,
            extra: None,
        };

        let handler =
            EventHandler::from_handler_definition("DEFAULT_ADMIN_ROLE".to_string(), definition)
                .unwrap();

        // Verify this is an AccessControl handler with add/remove operations
        assert!(handler.add_operation.is_some());
        assert!(handler.remove_operation.is_some());
        assert!(handler.set_operation.is_none());
        assert!(handler.group_by.is_none());

        // Verify it tracks role/account changes
        let add_op = handler.add_operation.as_ref().unwrap();
        assert_eq!(
            add_op.events(),
            &[
                "RoleGranted(bytes32 indexed role,address indexed account,address indexed sender)"
                    .to_string()
            ]
        );

        let remove_op = handler.remove_operation.as_ref().unwrap();
        assert_eq!(
            remove_op.events(),
            &[
                "RoleRevoked(bytes32 indexed role,address indexed account,address indexed sender)"
                    .to_string()
            ]
        );

        // Verify select fields (should return both role and account for basic accessControl)
        assert_eq!(handler.select_fields.len(), 2);
        assert_eq!(handler.select_fields[0], "role");
        assert_eq!(handler.select_fields[1], "account");

        // Optional integration test - this may return 0 events which is fine for unit testing
        maybe_execute_handler(
            &handler,
            None,
            "0xb3c415c2Aad428D5570208e1772cb68e7D06a537",
            "adminChanged",
        );
    }

    #[test]
    fn test_access_control_handler_variants() {
        // Test 1: Specific role AccessControl handler (pick_role_members = Some(role))
        let specific_definition = HandlerDefinition::AccessControl {
            role_names: Some(HashMap::from([(
                "MINTER_ROLE".to_string(),
                "keccek256(MINTER_ROLE)".to_string(),
            )])),
            pick_role_members: Some("MINTER_ROLE".to_string()),
            ignore_relative: None,
            extra: None,
        };

        println!("{:?}", specific_definition);
        let hash = keccak256("MINTER_ROLE").to_string();

        let specific_handler = EventHandler::<AnyNetwork>::from_handler_definition(
            "minters".to_string(),
            specific_definition,
        )
        .unwrap();

        // Verify it has add/remove operations with role filtering
        let add_op = specific_handler.add_operation.as_ref().unwrap();
        assert_eq!(
            add_op.events(),
            &[
                "RoleGranted(bytes32 indexed role,address indexed account,address indexed sender)"
                    .to_string()
            ]
        );
        assert_eq!(
            add_op.where_clause.as_ref().unwrap(),
            &json!(["=", "role", hash.clone()])
        );

        let remove_op = specific_handler.remove_operation.as_ref().unwrap();
        assert_eq!(
            remove_op.events(),
            &[
                "RoleRevoked(bytes32 indexed role,address indexed account,address indexed sender)"
                    .to_string()
            ]
        );
        assert_eq!(
            remove_op.where_clause.as_ref().unwrap(),
            &json!(["=", "role", hash.clone()])
        );

        // Verify select fields (should return only account for specific role)
        assert_eq!(specific_handler.select_fields.len(), 1);
        assert_eq!(specific_handler.select_fields[0], "account");

        // Verify other fields
        assert!(specific_handler.set_operation.is_none());
        assert!(specific_handler.group_by.is_none());
    }

    #[test]
    fn test_deserialize_kroma_upgraded_event_handler() {
        // Create a handler definition for Upgraded events
        let definition = HandlerDefinition::Event {
            event: Some("Upgraded(address)".to_string()),
            return_type: None,
            select: Some(serde_json::Value::String("implementation".to_string())),
            add: None,
            remove: None,
            set: None,
            group_by: None,
            ignore_relative: None,
        };

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition("upgraded".to_string(), definition)
                .unwrap();

        // Verify the event signature was parsed correctly
        assert_eq!(handler.event_signatures.len(), 1);
        assert_eq!(handler.event_signatures[0], "Upgraded(address)");

        // Verify select fields
        assert_eq!(handler.select_fields.len(), 1);
        assert_eq!(handler.select_fields[0], "implementation");

        // Verify this is a simple Event handler (no add/remove/set operations)
        assert!(handler.add_operation.is_none());
        assert!(handler.remove_operation.is_none());
        assert!(handler.set_operation.is_none());
        assert!(handler.group_by.is_none());
    }

    #[test]
    fn test_deserialize_set_operation_handler() {
        // Create a handler definition for set operations (like RoleGranted with specific filtering)
        let definition = HandlerDefinition::Event {
            event: None,
            return_type: None,
            select: Some(serde_json::Value::Array(vec![serde_json::Value::String(
                "delay".to_string(),
            )])),
            add: None,
            remove: None,
            set: Some(EventOperation {
                event: vec!["RoleGranted".to_string()],
                where_clause: Some(serde_json::json!([
                    "and",
                    ["=", "account", "0x28fC10E12A78f986c78F973Fc70ED88072b34c8e"],
                    ["=", "roleId", "565311800027786426"]
                ])),
            }),
            group_by: None,
            ignore_relative: None,
        };

        let handler = EventHandler::<AnyNetwork>::from_handler_definition(
            "setHandler".to_string(),
            definition,
        )
        .unwrap();

        // Verify the set operation was parsed correctly
        assert!(handler.set_operation.is_some());
        assert!(handler.add_operation.is_none());
        assert!(handler.remove_operation.is_none());
        assert!(handler.group_by.is_none());

        let set_op = handler.set_operation.as_ref().unwrap();
        assert_eq!(set_op.events(), &["RoleGranted".to_string()]);
        assert!(set_op.where_clause.is_some());

        // Verify select fields
        assert_eq!(handler.select_fields.len(), 1);
        assert_eq!(handler.select_fields[0], "delay");
    }

    #[test]
    fn test_deserialize_group_by_handler() {
        // Create a handler definition with groupBy (like RoleGuardianChanged grouped by roleId)
        let definition = HandlerDefinition::Event {
            event: None,
            return_type: None,
            select: Some(serde_json::Value::String("guardian".to_string())),
            add: None,
            remove: None,
            set: Some(EventOperation {
                event: vec!["RoleGuardianChanged".to_string()],
                where_clause: None,
            }),
            group_by: Some("roleId".to_string()),
            ignore_relative: None,
        };

        let handler = EventHandler::<AnyNetwork>::from_handler_definition(
            "groupByHandler".to_string(),
            definition,
        )
        .unwrap();

        // Verify the groupBy was parsed correctly
        assert!(handler.group_by.is_some());
        assert_eq!(handler.group_by.as_ref().unwrap(), "roleId");

        // Verify set operation
        assert!(handler.set_operation.is_some());
        let set_op = handler.set_operation.as_ref().unwrap();
        assert_eq!(set_op.events(), &["RoleGuardianChanged".to_string()]);

        // Verify select fields
        assert_eq!(handler.select_fields.len(), 1);
        assert_eq!(handler.select_fields[0], "guardian");
    }

    #[test]
    fn test_deserialize_mixed_operations_with_group_by() {
        // Create a handler definition with add operations and groupBy (like RolesGranted grouped by roleId)
        let definition = HandlerDefinition::Event {
            event: None,
            return_type: None,
            select: Some(serde_json::Value::Array(vec![
                serde_json::Value::String("account".to_string()),
                serde_json::Value::String("delay".to_string()),
                serde_json::Value::String("since".to_string()),
                serde_json::Value::String("newMember".to_string()),
            ])),
            add: Some(EventOperation {
                event: vec!["RoleGranted".to_string()],
                where_clause: None,
            }),
            remove: None,
            set: None,
            group_by: Some("roleId".to_string()),
            ignore_relative: None,
        };

        let handler = EventHandler::<AnyNetwork>::from_handler_definition(
            "mixedGroupByHandler".to_string(),
            definition,
        )
        .unwrap();

        // Verify the operations and groupBy
        assert!(handler.add_operation.is_some());
        assert!(handler.remove_operation.is_none());
        assert!(handler.set_operation.is_none());
        assert!(handler.group_by.is_some());
        assert_eq!(handler.group_by.as_ref().unwrap(), "roleId");

        // Verify add operation
        let add_op = handler.add_operation.as_ref().unwrap();
        assert_eq!(add_op.events(), &["RoleGranted".to_string()]);

        // Verify select fields
        assert_eq!(handler.select_fields.len(), 4);
        assert_eq!(
            handler.select_fields,
            vec!["account", "delay", "since", "newMember"]
        );
    }
}
