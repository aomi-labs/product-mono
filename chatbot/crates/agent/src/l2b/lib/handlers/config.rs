use serde::{
    de::{Deserializer, Error as DeError, SeqAccess, Visitor},
    ser::{SerializeSeq, Serializer},
    Deserialize, Serialize,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::fmt;

// =============================================================================
// TOP-LEVEL CONFIGURATION STRUCTS
// =============================================================================

/// Main discovery configuration for a project
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryConfig {
    pub name: String,
    pub chain: String,
    pub initial_addresses: Vec<String>,
    pub import: Option<Vec<String>>,
    pub max_addresses: Option<u64>,
    pub max_depth: Option<u64>,
    pub overrides: Option<HashMap<String, ContractConfig>>,
    pub shared_modules: Option<Vec<String>>,
    pub types: Option<HashMap<String, CustomType>>,
}

/// Configuration for a specific contract
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContractConfig {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub extends: Option<String>,
    pub can_act_independently: Option<bool>,
    pub ignore_discovery: Option<serde_json::Value>, // Can be bool or string (address)
    pub proxy_type: Option<String>,
    pub ignore_in_watch_mode: Option<Vec<String>>,
    pub ignore_methods: Option<Vec<String>>,
    pub ignore_relatives: Option<Vec<String>>,
    pub fields: Option<HashMap<String, ContractField>>,
    pub methods: Option<HashMap<String, String>>,
    pub manual_source_paths: Option<HashMap<String, String>>,
    pub types: Option<HashMap<String, CustomType>>,
    pub references: Option<Vec<ExternalReference>>,
}

// =============================================================================
// CONTRACT FIELD DEFINITIONS
// =============================================================================

/// Configuration for a field within a contract
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContractField {
    pub handler: Option<HandlerDefinition>,
    pub template: Option<String>,
    pub copy: Option<String>,
    pub permissions: Option<Vec<Permission>>,
    pub description: Option<String>,
    pub severity: Option<String>,
    #[serde(rename = "type")]
    pub field_type: Option<String>,
    pub edit: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: Option<HashMap<String, serde_json::Value>>,
}

/// Handler definitions for different types of data extraction
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HandlerDefinition {
    /// Storage handler - reads directly from contract storage slots
    Storage {
        slot: Option<serde_json::Value>,
        offset: Option<u64>,
        #[serde(rename = "returnType")]
        return_type: Option<String>,
        ignore_relative: Option<bool>,
    },
    /// Call handler - calls a view/pure function on the contract
    Call {
        method: String,
        args: Option<Vec<serde_json::Value>>,
        ignore_relative: Option<bool>,
        expect_revert: Option<bool>,
        address: Option<String>,
    },
    /// Event handler - reconstructs state from historical events
    Event {
        event: Option<String>,
        return_type: Option<String>,
        select: Option<serde_json::Value>,
        add: Option<EventOperation>,
        remove: Option<EventOperation>,
        set: Option<EventOperation>,
        #[serde(rename = "groupBy")]
        group_by: Option<String>,
        ignore_relative: Option<bool>,
    },
    /// Array handler - iterates through an array using indexed access
    Array {
        method: Option<String>,
        max_length: Option<u64>,
        #[serde(rename = "returnType")]
        return_type: Option<String>,
        indices: Option<serde_json::Value>,
        length: Option<serde_json::Value>,
        start_index: Option<u64>,
        ignore_relative: Option<bool>,
    },
    /// DynamicArray handler - reads dynamic arrays from storage
    DynamicArray {
        slot: Option<serde_json::Value>,
        #[serde(rename = "returnType")]
        return_type: Option<String>,
        ignore_relative: Option<bool>,
    },
    /// AccessControl handler - extracts OpenZeppelin AccessControl roles and members
    AccessControl {
        role_names: Option<HashMap<String, String>>,
        pick_role_members: Option<String>,
        ignore_relative: Option<bool>,
        #[serde(flatten)]
        extra: Option<HashMap<String, serde_json::Value>>,
    },
    /// Hardcoded handler - returns a static value
    Hardcoded {
        value: serde_json::Value,
    },
    /// EIP-2535 Diamond Facets handler - extracts facet addresses and function selectors
    #[serde(rename = "eip2535Facets")]
    Eip2535Facets {},
    /// EventCount handler - counts occurrences of events matching topic filters
    #[serde(rename = "eventCount")]
    EventCount {
        topics: Option<Vec<Option<String>>>,
        #[serde(flatten)]
        extra: Option<HashMap<String, serde_json::Value>>,
    },
    /// ConstructorArgs handler - extracts constructor arguments from creation transaction
    #[serde(rename = "constructorArgs")]
    ConstructorArgs {},

    // Platform-specific handlers
    /// Arbitrum Scheduled Transactions handler
    #[serde(rename = "arbitrumScheduledTransactions")]
    ArbitrumScheduledTransactions {},
    /// Arbitrum Actors handler (sequencer, validators, etc.)
    #[serde(rename = "arbitrumActors")]
    ArbitrumActors {
        actor_type: Option<String>,
        #[serde(flatten)]
        extra: Option<HashMap<String, serde_json::Value>>,
    },
    /// Arbitrum DAC Keyset handler
    #[serde(rename = "arbitrumDACKeyset")]
    ArbitrumDACKeyset {},
    /// Arbitrum Sequencer Version handler
    #[serde(rename = "arbitrumSequencerVersion")]
    ArbitrumSequencerVersion {},
    /// Scroll AccessControl handler (platform-specific variant)
    #[serde(rename = "scrollAccessControl")]
    ScrollAccessControl {},
    /// StarkWare Named Storage handler
    #[serde(rename = "starkWareNamedStorage")]
    StarkWareNamedStorage {},
    /// Linea Roles Module handler
    #[serde(rename = "lineaRolesModule")]
    LineaRolesModule {},
    /// Polygon CDK Scheduled Transactions handler
    #[serde(rename = "polygoncdkScheduledTransactions")]
    PolygoncdkScheduledTransactions {},
    /// OP Stack Data Availability handler
    #[serde(rename = "opStackDA")]
    OpStackDA {},
    /// zkSync Era Validators handler
    #[serde(rename = "zksynceraValidators")]
    ZksynceraValidators {},
    /// Kinto AccessControl handler
    #[serde(rename = "kintoAccessControl")]
    KintoAccessControl {},
    /// OP Stack Sequencer Inbox handler
    #[serde(rename = "opStackSequencerInbox")]
    OpStackSequencerInbox {},
    /// Orbit Posts Blobs handler
    #[serde(rename = "orbitPostsBlobs")]
    OrbitPostsBlobs {},
}

// =============================================================================
// SUPPORTING STRUCTS
// =============================================================================

/// Event operation for add/remove actions in event handlers
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventOperation {
    #[serde(
        deserialize_with = "deserialize_event_field",
        serialize_with = "serialize_event_field"
    )]
    pub event: Vec<String>,
    #[serde(rename = "where")]
    pub where_clause: Option<serde_json::Value>,
}

/// Permission definition for contract operations
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    #[serde(rename = "type")]
    pub permission_type: String,
    pub description: Option<String>,
    pub target: Option<String>,
    pub via: Option<Vec<PermissionVia>>,
}

/// Via clause for permissions (delegation path)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionVia {
    pub address: Option<String>,
    pub description: Option<String>,
}

/// External reference (documentation links, etc.)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReference {
    pub text: Option<String>,
    pub href: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
}

/// Custom type definition
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CustomType {
    pub type_name: Option<String>,
    pub description: Option<String>,
}

// =============================================================================
// PARSING FUNCTIONS
// =============================================================================

/// Parse a JSONC config file into a ContractConfig struct
pub fn parse_config_file(path: &Path) -> Result<ContractConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;

    // Parse JSONC to JSON
    let parsed = jsonc_parser::parse_to_value(&content, &Default::default())?;

    // Convert to serde_json::Value
    let json_value = match parsed {
        Some(value) => jsonc_to_serde_value(value),
        None => serde_json::Value::Null,
    };

    // Convert to our struct
    let config: ContractConfig = serde_json::from_value(json_value)?;

    Ok(config)
}

/// Convert jsonc_parser::JsonValue to serde_json::Value
pub fn jsonc_to_serde_value(value: jsonc_parser::JsonValue) -> serde_json::Value {
    match value {
        jsonc_parser::JsonValue::Null => serde_json::Value::Null,
        jsonc_parser::JsonValue::Boolean(b) => serde_json::Value::Bool(b),
        jsonc_parser::JsonValue::Number(n) => {
            if let Ok(i) = n.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(i))
            } else if let Ok(f) = n.parse::<f64>() {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
                )
            } else {
                serde_json::Value::Null
            }
        }
        jsonc_parser::JsonValue::String(s) => serde_json::Value::String(s.to_string()),
        jsonc_parser::JsonValue::Array(arr) => {
            let values: Vec<serde_json::Value> =
                arr.into_iter().map(jsonc_to_serde_value).collect();
            serde_json::Value::Array(values)
        }
        jsonc_parser::JsonValue::Object(obj) => {
            let map: serde_json::Map<String, serde_json::Value> = obj
                .into_iter()
                .map(|(k, v)| (k.to_string(), jsonc_to_serde_value(v)))
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Get a string representation of a handler type for analysis
pub fn get_handler_type_name(handler: &HandlerDefinition) -> &'static str {
    match handler {
        HandlerDefinition::Storage { .. } => "storage",
        HandlerDefinition::Call { .. } => "call",
        HandlerDefinition::Event { .. } => "event",
        HandlerDefinition::Array { .. } => "array",
        HandlerDefinition::DynamicArray { .. } => "dynamicArray",
        HandlerDefinition::AccessControl { .. } => "accessControl",
        HandlerDefinition::Hardcoded { .. } => "hardcoded",
        HandlerDefinition::Eip2535Facets { .. } => "eip2535Facets",
        HandlerDefinition::EventCount { .. } => "eventCount",
        HandlerDefinition::ConstructorArgs { .. } => "constructorArgs",

        HandlerDefinition::ArbitrumScheduledTransactions { .. } => "arbitrumScheduledTransactions",
        HandlerDefinition::ArbitrumActors { .. } => "arbitrumActors",
        HandlerDefinition::ScrollAccessControl { .. } => "scrollAccessControl",
        HandlerDefinition::StarkWareNamedStorage { .. } => "starkWareNamedStorage",
        HandlerDefinition::LineaRolesModule { .. } => "lineaRolesModule",
        HandlerDefinition::PolygoncdkScheduledTransactions { .. } => {
            "polygoncdkScheduledTransactions"
        }
        HandlerDefinition::OpStackDA { .. } => "opStackDA",
        HandlerDefinition::ArbitrumDACKeyset { .. } => "arbitrumDACKeyset",
        HandlerDefinition::ZksynceraValidators { .. } => "zksynceraValidators",
        HandlerDefinition::KintoAccessControl { .. } => "kintoAccessControl",
        HandlerDefinition::OpStackSequencerInbox { .. } => "opStackSequencerInbox",
        HandlerDefinition::OrbitPostsBlobs { .. } => "orbitPostsBlobs",
        HandlerDefinition::ArbitrumSequencerVersion { .. } => "arbitrumSequencerVersion",
    }
}

impl EventOperation {
    pub fn events(&self) -> &[String] {
        &self.event
    }

    pub fn primary_event(&self) -> Option<&str> {
        self.event.first().map(|s| s.as_str())
    }

    pub fn sanitize(mut self) -> Self {
        if let Some(where_clause) = self.where_clause.take() {
            self.where_clause = Some(sanitize_where_clause(where_clause));
        }
        println!("sanitize {:?}", self.where_clause);
        self
    }
}

fn deserialize_event_field<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct EventVisitor;

    impl<'de> Visitor<'de> for EventVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string event name or an array of event names")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(vec![value.to_owned()])
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(vec![value])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::new();

            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }

            if values.is_empty() {
                return Err(DeError::invalid_length(0, &"at least one event string"));
            }

            Ok(values)
        }
    }

    deserializer.deserialize_any(EventVisitor)
}

fn serialize_event_field<S>(events: &[String], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if events.len() == 1 {
        serializer.serialize_str(&events[0])
    } else {
        let mut seq = serializer.serialize_seq(Some(events.len()))?;
        for event in events {
            seq.serialize_element(event)?;
        }
        seq.end()
    }
}

fn sanitize_where_clause(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            if let Some(stripped) = s.strip_prefix('#') {
                serde_json::Value::String(stripped.to_string())
            } else {
                serde_json::Value::String(s)
            }
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sanitize_where_clause).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, sanitize_where_clause(v)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use walkdir::WalkDir;

    #[test]
    fn test_parse_config_file() {
        // Parse all files (original behavior)
        let config_dir = "src/discovery/projects"; // Parent directory to search for config files
        let mut config_files = Vec::new();
        let mut total_files = 0;

        // Find all .jsonc files
        for entry in WalkDir::new(config_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonc"))
        {
            total_files += 1;
            let path = entry.path();

            match parse_config_file(path) {
                Ok(config) => {
                    config_files.push((path.to_string_lossy().to_string(), config));
                    println!("✓ Parsed: {}", path.display());
                }
                Err(e) => {
                    eprintln!("✗ Failed to parse {}: {}", path.display(), e);
                }
            }
        }

        println!("\n=== SUMMARY ===");
        println!("Total .jsonc files found: {}", total_files);
        println!("Successfully parsed: {}", config_files.len());
        println!("Failed to parse: {}", total_files - config_files.len());

        // Analyze the parsed configs
        analyze_configs(&config_files);
    }

    fn analyze_configs(configs: &[(String, ContractConfig)]) {
        let mut schemas = HashMap::new();
        let mut categories = HashMap::new();
        let mut ignore_patterns = HashMap::new();
        let mut permission_types = HashMap::new();
        let mut handler_types = HashMap::new();

        for (_path, config) in configs {
            // Count schemas
            if let Some(schema) = &config.schema {
                *schemas.entry(schema.clone()).or_insert(0) += 1;
            }

            // Count categories
            if let Some(category) = &config.category {
                *categories.entry(category.clone()).or_insert(0) += 1;
            }

            // Count ignore patterns
            if let Some(ignore) = &config.ignore_in_watch_mode {
                for pattern in ignore {
                    *ignore_patterns.entry(pattern.clone()).or_insert(0) += 1;
                }
            }

            // Count permission types
            if let Some(fields) = &config.fields {
                for field in fields.values() {
                    if let Some(permissions) = &field.permissions {
                        for permission in permissions {
                            *permission_types
                                .entry(permission.permission_type.clone())
                                .or_insert(0) += 1;
                        }
                    }
                }
            }

            // Count handler types
            if let Some(fields) = &config.fields {
                for field in fields.values() {
                    if let Some(handler) = &field.handler {
                        let handler_type = get_handler_type_name(handler);
                        *handler_types.entry(handler_type.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }

        println!("\n=== SCHEMAS ===");
        for (schema, count) in schemas {
            println!("  {}: {} files", schema, count);
        }

        println!("\n=== CATEGORIES ===");
        let mut sorted_categories: Vec<_> = categories.into_iter().collect();
        sorted_categories.sort_by(|a, b| b.1.cmp(&a.1));
        for (category, count) in sorted_categories {
            println!("  {}: {} files", category, count);
        }

        println!("\n=== IGNORE PATTERNS ===");
        let mut sorted_patterns: Vec<_> = ignore_patterns.into_iter().collect();
        sorted_patterns.sort_by(|a, b| b.1.cmp(&a.1));
        for (pattern, count) in sorted_patterns.into_iter().take(10) {
            println!("  {}: {} files", pattern, count);
        }

        println!("\n=== PERMISSION TYPES ===");
        let mut sorted_permissions: Vec<_> = permission_types.into_iter().collect();
        sorted_permissions.sort_by(|a, b| b.1.cmp(&a.1));
        for (perm_type, count) in sorted_permissions {
            println!("  {}: {} occurrences", perm_type, count);
        }

        println!("\n=== HANDLER TYPES ===");
        let mut sorted_handlers: Vec<_> = handler_types.into_iter().collect();
        sorted_handlers.sort_by(|a, b| b.1.cmp(&a.1));
        for (handler_type, count) in sorted_handlers {
            println!("  {}: {} occurrences", handler_type, count);
        }
    }
}
