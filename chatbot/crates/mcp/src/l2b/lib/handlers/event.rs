use alloy_primitives::{Address, B256, hex};
use alloy_provider::{Provider, RootProvider, network::Network};
use alloy_rpc_types::Log;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use super::config::{EventOperation, HandlerDefinition};
use super::types::{Handler, HandlerResult, HandlerValue};

/// EventHandler fetches and processes historical events from a contract
///
/// Supports three modes:
/// 1. `set` - Returns the most recent matching event
/// 2. `add`/`remove` - Tracks a set of items by processing add/remove events
/// 3. Direct fetch - Returns all matching events
pub struct EventHandler<N: Network> {
    field: String,
    event_signatures: Vec<String>,         // Event signatures to match
    select_fields: Vec<String>,            // Fields to extract from events
    add_operation: Option<EventOperation>, // Add event configuration
    remove_operation: Option<EventOperation>, // Remove event configuration
    dependencies: Vec<String>,
    hidden: bool,
    _phantom: PhantomData<N>,
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
                ..
            } => {
                // Parse event signatures
                let event_signatures = if let Some(event_str) = event {
                    vec![event_str]
                } else {
                    return Err(anyhow!("Event handler requires 'event' field"));
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

                Ok(Self {
                    field,
                    event_signatures,
                    select_fields,
                    add_operation: add,
                    remove_operation: remove,
                    dependencies: vec![],
                    hidden: false,
                    _phantom: PhantomData,
                })
            }
            _ => Err(anyhow!("Expected Event handler definition")),
        }
    }

    /// Compute topic0 (event signature hash) from event signature string
    /// Format: "EventName(type1,type2,...)" or "EventName(type1 name1, type2 name2, ...)"
    /// Names are stripped before hashing to get the canonical signature
    fn compute_topic0(event_sig: &str) -> B256 {
        use alloy_primitives::keccak256;

        // Strip parameter names to get canonical signature
        // "Transfer(address from, address to, uint256 value)" -> "Transfer(address,address,uint256)"
        let canonical_sig = Self::strip_parameter_names(event_sig);

        keccak256(canonical_sig.as_bytes())
    }

    /// Strip parameter names from event signature to get canonical form
    /// "Transfer(address from, address to, uint256 value)" -> "Transfer(address,address,uint256)"
    fn strip_parameter_names(event_sig: &str) -> String {
        if let Some(start) = event_sig.find('(') {
            if let Some(end) = event_sig.rfind(')') {
                let event_name = &event_sig[..start];
                let params_str = &event_sig[start + 1..end];

                // If empty params, return as-is
                if params_str.trim().is_empty() {
                    return event_sig.to_string();
                }

                // Split by comma and extract only types
                let types: Vec<String> = params_str
                    .split(',')
                    .map(|param| {
                        let param = param.trim();
                        // Take only the first word (the type)
                        param.split_whitespace().next().unwrap_or(param).to_string()
                    })
                    .collect();

                return format!("{}({})", event_name, types.join(","));
            }
        }

        // If parsing fails, return original
        event_sig.to_string()
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

    /// Fetch logs for the given event signatures
    async fn fetch_logs(&self, provider: &RootProvider<N>, address: &Address) -> Result<Vec<Log>> {
        let mut all_logs = Vec::new();

        // Get current block number to determine range
        let to_block = provider
            .get_block_number()
            .await
            .map_err(|e| anyhow!("Failed to get current block number: {}", e))?;

        // Query last 10,000 blocks by default (to avoid timeout on full history)
        let from_block = to_block.saturating_sub(10_000);

        for event_sig in &self.event_signatures {
            let topic0 = Self::compute_topic0(event_sig);
            let logs =
                Self::fetch_logs_batched(provider, address, topic0, from_block, to_block).await?;
            all_logs.extend(logs);
        }

        // Sort logs by block number and log index
        all_logs.sort_by(|a, b| match a.block_number.cmp(&b.block_number) {
            std::cmp::Ordering::Equal => a.log_index.cmp(&b.log_index),
            other => other,
        });

        Ok(all_logs)
    }

    /// Parse a log into a HashMap of field values
    /// This is a simplified parser - in production you'd use the full ABI
    fn parse_log(&self, log: &Log) -> Result<HashMap<String, HandlerValue>> {
        let mut result = HashMap::new();

        // Topics: [topic0 (event sig), topic1 (indexed param 1), topic2, topic3]
        let topics = log.topics();

        // Extract event signature to try to get field names
        let event_sig = self.event_signatures.first();
        let field_names = if let Some(sig) = event_sig {
            self.parse_event_field_names(sig)
        } else {
            Vec::new()
        };

        // Extract indexed parameters (topics 1-3)
        for (i, topic) in topics.iter().skip(1).enumerate() {
            // Use actual field name if available, otherwise use indexed_N
            let field_name = field_names
                .get(i)
                .and_then(|name| {
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.clone())
                    }
                })
                .unwrap_or_else(|| format!("indexed_{}", i));

            // Check if this could be a boolean (all zeros except last byte is 0 or 1)
            let is_boolean = topic.len() == 32
                && topic[0..31].iter().all(|&b| b == 0)
                && (topic[31] == 0 || topic[31] == 1);

            if is_boolean {
                // Store as boolean
                result.insert(field_name.clone(), HandlerValue::Boolean(topic[31] == 1));
            } else if topic.len() == 32 {
                // Try to interpret as address (last 20 bytes, ignoring first 12 zero bytes)
                let addr_bytes = &topic[12..32];
                // Only treat as address if first 12 bytes are zero (proper padding)
                if topic[0..12].iter().all(|&b| b == 0) {
                    if let Ok(addr) = Address::try_from(addr_bytes) {
                        result.insert(field_name.clone(), HandlerValue::Address(addr));
                        // Also store raw bytes in case it's needed
                        result.insert(
                            format!("{}_bytes", field_name),
                            HandlerValue::Bytes(topic.0.into()),
                        );
                        continue;
                    }
                }

                // Otherwise, it could be a uint256 or bytes32
                // Store as bytes and let the caller interpret
                result.insert(field_name.clone(), HandlerValue::Bytes(topic.0.into()));
            } else {
                // Unexpected topic length, store as bytes
                result.insert(field_name.clone(), HandlerValue::Bytes(topic.0.into()));
            }
        }

        // Store the raw data
        let log_data = log.data();
        if !log_data.data.is_empty() {
            result.insert(
                "data".to_string(),
                HandlerValue::Bytes(log_data.data.clone()),
            );

            // Try to decode non-indexed parameters from data field
            // For now, handle common cases like single bool or single uint256
            if log_data.data.len() == 32 {
                // Single 32-byte value
                let data_bytes = &log_data.data[..];

                // Check if it's a boolean (all zeros except last byte is 0 or 1)
                let is_boolean = data_bytes[0..31].iter().all(|&b| b == 0)
                    && (data_bytes[31] == 0 || data_bytes[31] == 1);

                if is_boolean {
                    // Get the non-indexed parameter name from signature
                    let non_indexed_names: Vec<String> = field_names
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i >= topics.len() - 1) // Parameters beyond topic count
                        .map(|(_, name)| name.clone())
                        .collect();

                    if let Some(param_name) = non_indexed_names.first() {
                        if !param_name.is_empty() {
                            result.insert(
                                param_name.clone(),
                                HandlerValue::Boolean(data_bytes[31] == 1),
                            );
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Parse event field names from event signature
    /// Supports two formats:
    /// 1. With names: "ProposerPermissionUpdated(address proposer, bool allowed)" -> ["proposer", "allowed"]
    /// 2. Without names: "ProposerPermissionUpdated(address,bool)" -> []
    fn parse_event_field_names(&self, event_sig: &str) -> Vec<String> {
        // Extract the parameters part: "EventName(params)" -> "params"
        if let Some(start) = event_sig.find('(') {
            if let Some(end) = event_sig.rfind(')') {
                let params_str = &event_sig[start + 1..end];

                // If empty, return empty vec
                if params_str.trim().is_empty() {
                    return Vec::new();
                }

                // Split by comma and parse each parameter
                let mut field_names = Vec::new();
                for param in params_str.split(',') {
                    let param = param.trim();

                    // Parse "type name" format
                    // e.g., "address proposer" -> "proposer"
                    // e.g., "uint256" -> (no name, skip)
                    let parts: Vec<&str> = param.split_whitespace().collect();

                    if parts.len() >= 2 {
                        // Has a name - use it
                        field_names.push(parts[1].to_string());
                    } else {
                        // No name provided - will use indexed_N fallback
                        field_names.push(String::new());
                    }
                }

                return field_names;
            }
        }

        Vec::new()
    }

    /// Extract selected fields from parsed event data
    fn extract_fields(&self, parsed: HashMap<String, HandlerValue>) -> HandlerValue {
        if self.select_fields.is_empty() {
            // Return all fields as object
            HandlerValue::Object(parsed)
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

    /// Process logs in "set" mode - return most recent matching event
    fn process_set_mode(&self, logs: Vec<Log>) -> Result<HandlerValue> {
        if logs.is_empty() {
            return Ok(HandlerValue::String("null".to_string()));
        }

        // Get the most recent log (last in sorted array)
        let most_recent = logs.last().unwrap();
        let parsed = self.parse_log(most_recent)?;

        Ok(self.extract_fields(parsed))
    }

    /// Process logs in "array" mode - return all matching events
    fn process_array_mode(&self, logs: Vec<Log>) -> Result<HandlerValue> {
        let mut results = Vec::new();

        for log in logs {
            let parsed = self.parse_log(&log)?;
            let extracted = self.extract_fields(parsed);
            results.push(extracted);
        }

        Ok(HandlerValue::Array(results))
    }

    /// Process logs in "add/remove" mode - track a set of items
    async fn process_add_remove_mode(
        &self,
        provider: &RootProvider<N>,
        address: &Address,
    ) -> Result<HandlerValue> {
        let add_op = self.add_operation.as_ref();
        let remove_op = self.remove_operation.as_ref();

        if add_op.is_none() && remove_op.is_none() {
            return Err(anyhow!("Add/remove mode requires at least one operation"));
        }

        // Get current block number to determine range
        let to_block = provider
            .get_block_number()
            .await
            .map_err(|e| anyhow!("Failed to get current block number: {}", e))?;

        // Query last 10,000 blocks by default (to avoid timeout on full history)
        let from_block = to_block.saturating_sub(10_000);

        // Fetch logs for both add and remove events
        let mut all_logs = Vec::new();

        if let Some(add_op) = add_op {
            let topic0 = Self::compute_topic0(&add_op.event);
            let logs =
                Self::fetch_logs_batched(provider, address, topic0, from_block, to_block).await?;
            all_logs.extend(logs);
        }

        if let Some(remove_op) = remove_op {
            let topic0 = Self::compute_topic0(&remove_op.event);
            let logs =
                Self::fetch_logs_batched(provider, address, topic0, from_block, to_block).await?;
            all_logs.extend(logs);
        }

        // Sort logs by block number and log index
        all_logs.sort_by(|a, b| match a.block_number.cmp(&b.block_number) {
            std::cmp::Ordering::Equal => a.log_index.cmp(&b.log_index),
            other => other,
        });

        // Compute topic0 hashes for add/remove events
        let add_topic0 = add_op.map(|op| Self::compute_topic0(&op.event));
        let remove_topic0 = remove_op.map(|op| Self::compute_topic0(&op.event));

        // Track unique items in a set
        let mut tracked_set = HashSet::new();

        // Process logs chronologically (already sorted)
        for log in all_logs {
            let topic0 = log.topics().first();
            if topic0.is_none() {
                continue;
            }
            let topic0 = topic0.unwrap();

            // Parse the log to extract field values
            let parsed = self.parse_log(&log)?;

            // Check if this is an add event
            if let Some(add_t0) = add_topic0 {
                if topic0 == &add_t0 {
                    // Evaluate where clause if present
                    let should_add = if let Some(add_op) = add_op {
                        if let Some(where_clause) = &add_op.where_clause {
                            match self.evaluate_where_clause(where_clause, &parsed) {
                                Ok(result) => result,
                                Err(e) => {
                                    // Log error but continue processing
                                    eprintln!("Failed to evaluate add where clause: {}", e);
                                    false
                                }
                            }
                        } else {
                            true // No where clause, always add
                        }
                    } else {
                        true
                    };

                    if should_add {
                        let value = self.extract_fields(parsed);
                        let key = self.value_to_string(&value);
                        tracked_set.insert(key);
                    }
                    continue;
                }
            }

            // Check if this is a remove event
            if let Some(remove_t0) = remove_topic0 {
                if topic0 == &remove_t0 {
                    // Evaluate where clause if present
                    let should_remove = if let Some(remove_op) = remove_op {
                        if let Some(where_clause) = &remove_op.where_clause {
                            match self.evaluate_where_clause(where_clause, &parsed) {
                                Ok(result) => result,
                                Err(e) => {
                                    // Log error but continue processing
                                    eprintln!("Failed to evaluate remove where clause: {}", e);
                                    false
                                }
                            }
                        } else {
                            true // No where clause, always remove
                        }
                    } else {
                        true
                    };

                    if should_remove {
                        let value = self.extract_fields(parsed);
                        let key = self.value_to_string(&value);
                        tracked_set.remove(&key);
                    }
                    continue;
                }
            }
        }

        // Convert set to array of values
        let results: Vec<HandlerValue> = tracked_set
            .into_iter()
            .map(|s| HandlerValue::String(s))
            .collect();

        Ok(HandlerValue::Array(results))
    }

    /// Convert a HandlerValue to a string key for HashSet tracking
    /// This allows deduplication of values
    fn value_to_string(&self, value: &HandlerValue) -> String {
        match value {
            HandlerValue::String(s) => s.clone(),
            HandlerValue::Address(addr) => format!("{:?}", addr),
            HandlerValue::Number(n) => n.to_string(),
            HandlerValue::Bytes(b) => format!("0x{}", hex::encode(b)),
            HandlerValue::Boolean(b) => b.to_string(),
            HandlerValue::Array(_) => serde_json::to_string(value).unwrap_or_default(),
            HandlerValue::Object(_) => serde_json::to_string(value).unwrap_or_default(),
            HandlerValue::Reference(r) => r.clone(),
        }
    }

    /// Evaluate a where clause against parsed event data
    /// Where clause format: ["operator", "field", "value"]
    /// Examples:
    ///   ["=", "#allowed", true]  - Check if 'allowed' field equals true
    ///   ["!=", "#proposer", "0x0000..."] - Check if 'proposer' is not zero address
    fn evaluate_where_clause(
        &self,
        where_clause: &serde_json::Value,
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
            "=" | "==" => Ok(self.values_equal(actual_value, expected_value)),
            "!=" => Ok(!self.values_equal(actual_value, expected_value)),
            _ => Err(anyhow!("Unsupported where clause operator: {}", operator)),
        }
    }

    /// Compare a HandlerValue with a JSON value for equality
    fn values_equal(&self, handler_value: &HandlerValue, json_value: &serde_json::Value) -> bool {
        match (handler_value, json_value) {
            (HandlerValue::Boolean(b), serde_json::Value::Bool(jb)) => b == jb,
            (HandlerValue::String(s), serde_json::Value::String(js)) => s == js,
            (HandlerValue::Number(n), serde_json::Value::Number(jn)) => {
                // Compare as strings to handle large numbers
                n.to_string() == jn.to_string()
            }
            (HandlerValue::Number(n), serde_json::Value::String(js)) => {
                // Allow comparing numbers with string representations
                n.to_string() == *js
            }
            (HandlerValue::Address(addr), serde_json::Value::String(js)) => {
                // Compare addresses (both should be hex strings)
                format!("{:?}", addr).to_lowercase() == js.to_lowercase()
            }
            (HandlerValue::Bytes(b), serde_json::Value::String(js)) => {
                // Compare bytes as hex strings
                let hex_str = format!("0x{}", hex::encode(b));
                hex_str.to_lowercase() == js.to_lowercase()
            }
            _ => false,
        }
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
        // Fetch logs
        let logs = match self.fetch_logs(provider, address).await {
            Ok(logs) => logs,
            Err(e) => {
                return HandlerResult {
                    field: self.field.clone(),
                    value: None,
                    error: Some(format!("Failed to fetch logs: {}", e)),
                    hidden: self.hidden,
                };
            }
        };

        // Determine which mode to use based on add/remove operations
        let value = if self.add_operation.is_some() || self.remove_operation.is_some() {
            // Use add/remove mode if operations are defined
            match self.process_add_remove_mode(provider, address).await {
                Ok(v) => v,
                Err(e) => {
                    return HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Failed to process add/remove: {}", e)),
                        hidden: self.hidden,
                    };
                }
            }
        } else {
            // Default to "set" mode (most recent event)
            match self.process_set_mode(logs) {
                Ok(v) => v,
                Err(e) => {
                    return HandlerResult {
                        field: self.field.clone(),
                        value: None,
                        error: Some(format!("Failed to process logs: {}", e)),
                        hidden: self.hidden,
                    };
                }
            }
        };

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

    #[test]
    fn test_compute_topic0() {
        // Test with known event signature (without names)
        let topic0 =
            EventHandler::<AnyNetwork>::compute_topic0("Transfer(address,address,uint256)");

        // ERC20 Transfer event signature
        let expected = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
        assert_eq!(format!("{:?}", topic0), expected);

        // Test with names - should produce same hash (names are stripped)
        let topic0_with_names = EventHandler::<AnyNetwork>::compute_topic0(
            "Transfer(address from, address to, uint256 value)",
        );
        assert_eq!(format!("{:?}", topic0_with_names), expected);
    }

    #[test]
    fn test_strip_parameter_names() {
        assert_eq!(
            EventHandler::<AnyNetwork>::strip_parameter_names(
                "Transfer(address from, address to, uint256 value)"
            ),
            "Transfer(address,address,uint256)"
        );

        assert_eq!(
            EventHandler::<AnyNetwork>::strip_parameter_names(
                "ProposerPermissionUpdated(address proposer, bool allowed)"
            ),
            "ProposerPermissionUpdated(address,bool)"
        );

        // Already canonical - should not change
        assert_eq!(
            EventHandler::<AnyNetwork>::strip_parameter_names("Transfer(address,address,uint256)"),
            "Transfer(address,address,uint256)"
        );

        // Empty params
        assert_eq!(
            EventHandler::<AnyNetwork>::strip_parameter_names("NoParams()"),
            "NoParams()"
        );
    }

    #[test]
    fn test_parse_event_field_names() {
        let definition = HandlerDefinition::Event {
            event: Some("ProposerPermissionUpdated(address proposer, bool allowed)".to_string()),
            return_type: None,
            select: None,
            add: None,
            remove: None,
            ignore_relative: None,
        };

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition("test".to_string(), definition)
                .unwrap();

        let field_names = handler
            .parse_event_field_names("ProposerPermissionUpdated(address proposer, bool allowed)");

        assert_eq!(field_names.len(), 2);
        assert_eq!(field_names[0], "proposer");
        assert_eq!(field_names[1], "allowed");
    }

    #[test]
    fn test_parse_event_field_names_without_names() {
        let handler = EventHandler::<AnyNetwork> {
            field: "test".to_string(),
            event_signatures: vec!["Transfer(address,address,uint256)".to_string()],
            select_fields: vec![],
            add_operation: None,
            remove_operation: None,
            dependencies: vec![],
            hidden: false,
            _phantom: PhantomData,
        };

        let field_names = handler.parse_event_field_names("Transfer(address,address,uint256)");

        // Should return empty strings for unnamed parameters
        assert_eq!(field_names.len(), 3);
        assert_eq!(field_names[0], "");
        assert_eq!(field_names[1], "");
        assert_eq!(field_names[2], "");
    }

    #[test]
    fn test_parse_event_field_names_mixed() {
        let handler = EventHandler::<AnyNetwork> {
            field: "test".to_string(),
            event_signatures: vec!["Event(address sender, uint256, bool flag)".to_string()],
            select_fields: vec![],
            add_operation: None,
            remove_operation: None,
            dependencies: vec![],
            hidden: false,
            _phantom: PhantomData,
        };

        let field_names =
            handler.parse_event_field_names("Event(address sender, uint256, bool flag)");

        assert_eq!(field_names.len(), 3);
        assert_eq!(field_names[0], "sender");
        assert_eq!(field_names[1], ""); // No name for uint256
        assert_eq!(field_names[2], "flag");
    }

    #[test]
    fn test_from_handler_definition() {
        let definition = HandlerDefinition::Event {
            event: Some("Transfer(address,address,uint256)".to_string()),
            return_type: None,
            select: Some(serde_json::json!(["from", "to", "value"])),
            add: None,
            remove: None,
            ignore_relative: None,
        };

        let handler = EventHandler::<AnyNetwork>::from_handler_definition(
            "transfers".to_string(),
            definition,
        );

        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.field, "transfers");
        assert_eq!(handler.event_signatures.len(), 1);
        assert_eq!(handler.select_fields.len(), 3);
    }

    #[test]
    fn test_from_handler_definition_single_select() {
        let definition = HandlerDefinition::Event {
            event: Some("OwnershipTransferred(address,address)".to_string()),
            return_type: None,
            select: Some(serde_json::json!("newOwner")),
            add: None,
            remove: None,
            ignore_relative: None,
        };

        let handler =
            EventHandler::<AnyNetwork>::from_handler_definition("owner".to_string(), definition);

        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.select_fields.len(), 1);
        assert_eq!(handler.select_fields[0], "newOwner");
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test test_event_handler_execution --ignored -- --nocapture
    async fn test_event_handler_execution() {
        use alloy_provider::RootProvider;
        use std::str::FromStr;

        // Contract address: Taiko Protocol L1 contract
        // eth:0x686E7d01C7BFCB563721333A007699F154C04eb4
        let contract_address =
            Address::from_str("0x686E7d01C7BFCB563721333A007699F154C04eb4").expect("Valid address");

        // Create provider pointing to localhost:8545 (Anvil/Hardhat fork)
        let rpc_url = "https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362".parse().expect("Valid RPC URL");
        let provider = RootProvider::<alloy_provider::network::Ethereum>::new_http(rpc_url);

        // Create EventHandler with add/remove operations for whitelistedProposers
        // Note: proposer is indexed (topic1), allowed is NOT indexed (in data field)
        let handler = EventHandler::<alloy_provider::network::Ethereum>::from_handler_definition(
            "whitelistedProposers".to_string(),
            HandlerDefinition::Event {
                event: Some(
                    "ProposerPermissionUpdated(address indexed proposer,bool allowed)".to_string(),
                ),
                return_type: None,
                select: Some(serde_json::json!("proposer")),
                add: Some(crate::l2b::lib::handlers::config::EventOperation {
                    event: "ProposerPermissionUpdated(address indexed proposer,bool allowed)"
                        .to_string(),
                    where_clause: Some(serde_json::json!(["=", "allowed", true])),
                }),
                remove: Some(crate::l2b::lib::handlers::config::EventOperation {
                    event: "ProposerPermissionUpdated(address indexed proposer,bool allowed)"
                        .to_string(),
                    where_clause: Some(serde_json::json!(["=", "allowed", false])),
                }),
                ignore_relative: None,
            },
        )
        .expect("Failed to create EventHandler");

        println!("\n=== Testing EventHandler with Real Contract ===");
        println!("Contract: {}", contract_address);
        println!("Handler: whitelistedProposers");
        println!(
            "RPC:https://rpc.ankr.com/eth/2a9a32528f8a70a5b48c57e8fb83b4978f2a25c8368aa6fd9dc2f2321ae53362"
        );

        // First, let's manually check if we can fetch the logs
        println!("\n=== Manual Log Fetch Test ===");
        let topic0 = EventHandler::<alloy_provider::network::Ethereum>::compute_topic0(
            "ProposerPermissionUpdated(address indexed proposer,bool allowed)",
        );
        println!("Topic0: {:?}", topic0);

        // Add block range to avoid "query returned more than X results" errors
        // Event was at block 23119466, let's query a smaller range around it
        let filter = alloy_rpc_types::Filter::new()
            .address(contract_address)
            .event_signature(topic0)
            .from_block(23119000) // Start just before the known event
            .to_block(23120000); // End just after (1000 block range)

        match provider.get_logs(&filter).await {
            Ok(logs) => {
                println!("Fetched {} logs from mainnet", logs.len());
                for (i, log) in logs.iter().take(3).enumerate() {
                    println!("\n  Log {}:", i);
                    println!("    Block: {:?}", log.block_number);
                    println!("    Topics: {:?}", log.topics().len());
                    for (j, topic) in log.topics().iter().enumerate() {
                        println!("      Topic[{}]: {:?}", j, topic);
                    }
                    let log_data = log.data();
                    println!(
                        "    Data: {} bytes - 0x{}",
                        log_data.data.len(),
                        hex::encode(&log_data.data[..log_data.data.len().min(32)])
                    );
                }
            }
            Err(e) => {
                println!("Failed to fetch logs: {}", e);
            }
        }

        // Execute the handler
        println!("\n=== Executing Handler ===");
        let result = handler
            .execute(&provider, &contract_address, &HashMap::new())
            .await;

        println!("\n=== Execution Results ===");
        println!("Field: {}", result.field);
        println!("Error: {:?}", result.error);

        // Verify no errors
        assert!(
            result.error.is_none(),
            "Handler execution failed: {:?}",
            result.error
        );
        assert!(result.value.is_some(), "Expected a value from handler");

        // Check the result
        if let Some(value) = &result.value {
            match value {
                HandlerValue::Array(proposers) => {
                    println!("Whitelisted Proposers Count: {}", proposers.len());
                    for (i, proposer) in proposers.iter().enumerate() {
                        println!("  [{}] {:?}", i, proposer);
                    }

                    println!("\n✓ Successfully retrieved whitelisted proposers");
                }
                _ => panic!("Expected Array value, got: {:?}", value),
            }
        }

        println!("\n=== Integration Test Passed ===");
    }
}
