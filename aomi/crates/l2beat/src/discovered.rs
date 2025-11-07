use alloy_primitives::{Address, hex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::handlers::types::HandlerValue;

/// Top-level discovered.json structure compatible with L2Beat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredJson {
    pub name: String,
    pub timestamp: u64,
    #[serde(rename = "configHash")]
    pub config_hash: String,
    pub entries: Vec<DiscoveredContract>,
}

/// Individual contract entry in discovered.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredContract {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub address: String,
    #[serde(rename = "type")]
    pub contract_type: ContractType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "proxyType", skip_serializing_if = "Option::is_none")]
    pub proxy_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "sinceTimestamp", skip_serializing_if = "Option::is_none")]
    pub since_timestamp: Option<u64>,
    #[serde(rename = "sinceBlock", skip_serializing_if = "Option::is_none")]
    pub since_block: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractType {
    Contract,
    Eoa,
}

impl DiscoveredJson {
    /// Create a new discovered.json with the given name
    pub fn new(name: String) -> Self {
        Self {
            name,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            config_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            entries: Vec::new(),
        }
    }

    /// Add a contract entry to the discovered.json
    pub fn add_contract(
        &mut self,
        address: Address,
        name: Option<String>,
        values: HashMap<String, HandlerValue>,
        description: Option<String>,
    ) {
        // Convert HandlerValue to serde_json::Value
        let json_values: HashMap<String, serde_json::Value> = values
            .into_iter()
            .filter_map(|(key, value)| handler_value_to_json(&value).ok().map(|json| (key, json)))
            .collect();

        let entry = DiscoveredContract {
            name,
            address: format!("eth:{:?}", address),
            contract_type: ContractType::Contract,
            description,
            proxy_type: None, // TODO: Detect proxy type from values
            values: if json_values.is_empty() {
                None
            } else {
                Some(json_values)
            },
            since_timestamp: None,
            since_block: None,
        };

        self.entries.push(entry);
    }

    /// Write the discovered.json to a file
    pub fn write_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Convert HandlerValue to serde_json::Value for serialization
fn handler_value_to_json(value: &HandlerValue) -> Result<serde_json::Value, String> {
    match value {
        HandlerValue::String(s) => Ok(serde_json::Value::String(s.clone())),
        HandlerValue::Number(n) => {
            // Convert U256 to string to avoid precision loss
            Ok(serde_json::Value::String(n.to_string()))
        }
        HandlerValue::Address(addr) => Ok(serde_json::Value::String(format!("eth:{:?}", addr))),
        HandlerValue::Bytes(bytes) => Ok(serde_json::Value::String(format!(
            "0x{}",
            hex::encode(bytes)
        ))),
        HandlerValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        HandlerValue::Array(arr) => {
            let json_arr: Result<Vec<serde_json::Value>, String> =
                arr.iter().map(handler_value_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        HandlerValue::Object(obj) => {
            let json_obj: Result<serde_json::Map<String, serde_json::Value>, String> = obj
                .iter()
                .map(|(k, v)| handler_value_to_json(v).map(|json| (k.clone(), json)))
                .collect();
            Ok(serde_json::Value::Object(json_obj?))
        }
        HandlerValue::Reference(ref_str) => {
            // References should be resolved before serialization
            Err(format!("Unresolved reference: {}", ref_str))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_discovered_json_creation() {
        let discovered = DiscoveredJson::new("test-project".to_string());
        assert_eq!(discovered.name, "test-project");
        assert_eq!(discovered.entries.len(), 0);
    }

    #[test]
    fn test_add_contract() {
        let mut discovered = DiscoveredJson::new("test-project".to_string());

        let mut values = HashMap::new();
        values.insert(
            "owner".to_string(),
            HandlerValue::Address(Address::from([0x42; 20])),
        );
        values.insert(
            "totalSupply".to_string(),
            HandlerValue::Number(U256::from(1000000)),
        );

        discovered.add_contract(
            Address::from([0x11; 20]),
            Some("TestContract".to_string()),
            values,
            Some("A test contract".to_string()),
        );

        assert_eq!(discovered.entries.len(), 1);
        assert_eq!(discovered.entries[0].name, Some("TestContract".to_string()));
        assert!(discovered.entries[0].values.is_some());
    }

    #[test]
    fn test_handler_value_to_json() {
        // Test basic types
        assert_eq!(
            handler_value_to_json(&HandlerValue::String("test".to_string())).unwrap(),
            serde_json::Value::String("test".to_string())
        );

        assert_eq!(
            handler_value_to_json(&HandlerValue::Boolean(true)).unwrap(),
            serde_json::Value::Bool(true)
        );

        assert_eq!(
            handler_value_to_json(&HandlerValue::Number(U256::from(42))).unwrap(),
            serde_json::Value::String("42".to_string())
        );

        // Test address formatting
        let addr = Address::from([0x42; 20]);
        let json = handler_value_to_json(&HandlerValue::Address(addr)).unwrap();
        assert!(json.as_str().unwrap().starts_with("eth:0x"));

        // Test array
        let arr = HandlerValue::Array(vec![
            HandlerValue::Number(U256::from(1)),
            HandlerValue::Number(U256::from(2)),
        ]);
        let json = handler_value_to_json(&arr).unwrap();
        assert!(json.is_array());

        // Test reference error
        let ref_val = HandlerValue::Reference("{{ admin }}".to_string());
        assert!(handler_value_to_json(&ref_val).is_err());
    }
}
