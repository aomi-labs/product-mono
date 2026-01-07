use alloy_primitives::{B256, hex};
use serde_json::{self, Value};

use super::types::HandlerValue;

#[derive(Debug, Clone)]
pub struct EventParameter {
    pub typ: String,
    pub name: Option<String>,
    pub indexed: bool,
}

impl EventParameter {
    pub fn parse(param: &str) -> Option<Self> {
        let mut tokens: Vec<&str> = param
            .split_whitespace()
            .filter(|token| !token.is_empty())
            .collect();

        if tokens.is_empty() {
            return None;
        }

        let indexed = tokens.contains(&"indexed");
        tokens.retain(|token| *token != "indexed");

        if tokens.is_empty() {
            return None;
        }

        let typ = tokens[0].to_ascii_lowercase();
        let name = if tokens.len() > 1 {
            tokens.last().map(|token| token.to_string())
        } else {
            None
        };

        Some(Self { typ, name, indexed })
    }
}

pub fn parameter_section(event_signature: &str) -> Option<&str> {
    let start = event_signature.find('(')?;
    let end = event_signature.rfind(')')?;
    if end <= start {
        return None;
    }
    Some(&event_signature[start + 1..end])
}

/// Canonicalize event signature by removing parameter names and indexed keywords.
/// Example:
/// "Transfer(address indexed from, address to, uint256 value)" -> "Transfer(address,address,uint256)"
pub fn canonicalize_event_signature(event_signature: &str) -> String {
    let Some(start) = event_signature.find('(') else {
        return event_signature.to_string();
    };
    let Some(end) = event_signature.rfind(')') else {
        return event_signature.to_string();
    };

    let event_name = &event_signature[..start];
    let params_str = &event_signature[start + 1..end];

    if params_str.trim().is_empty() {
        return event_signature.to_string();
    }

    let mut types = Vec::new();
    for param in params_str.split(',') {
        match EventParameter::parse(param.trim()) {
            Some(EventParameter { typ, .. }) => types.push(typ),
            None => return event_signature.to_string(),
        }
    }

    format!("{}({})", event_name, types.join(","))
}

/// Compare a HandlerValue with a JSON value for equality.
pub fn values_equal(handler_value: &HandlerValue, json_value: &Value) -> bool {
    match (handler_value, json_value) {
        (HandlerValue::Boolean(b), Value::Bool(jb)) => b == jb,
        (HandlerValue::String(s), Value::String(js)) => s == js,
        (HandlerValue::Number(n), Value::Number(jn)) => n.to_string() == jn.to_string(),
        (HandlerValue::Number(n), Value::String(js)) => n.to_string() == *js,
        (HandlerValue::Address(addr), Value::String(js)) => {
            format!("{:?}", addr).to_lowercase() == js.to_lowercase()
        }
        (HandlerValue::Bytes(b), Value::String(js)) => {
            let hex_str = format!("0x{}", hex::encode(b));
            hex_str.to_lowercase() == js.to_lowercase()
        }
        _ => false,
    }
}

/// Encode a JSON value into a topic hash (B256).
#[allow(dead_code)]
pub fn encode_topic_value(value: &Value) -> Option<B256> {
    match value {
        Value::String(s) => {
            if let Some(stripped) = s.strip_prefix("0x") {
                if let Ok(decoded) = hex::decode(stripped) {
                    if decoded.len() <= 32 {
                        let mut bytes = [0u8; 32];
                        if decoded.len() == 20 {
                            bytes[12..32].copy_from_slice(&decoded);
                        } else {
                            bytes[32 - decoded.len()..].copy_from_slice(&decoded);
                        }
                        Some(B256::from(bytes))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                Some(alloy_primitives::keccak256(s.as_bytes()))
            }
        }
        Value::Number(n) => {
            if let Some(u64_val) = n.as_u64() {
                let mut bytes = [0u8; 32];
                bytes[24..32].copy_from_slice(&u64_val.to_be_bytes());
                Some(B256::from(bytes))
            } else {
                None
            }
        }
        Value::Bool(b) => {
            let mut bytes = [0u8; 32];
            bytes[31] = if *b { 1 } else { 0 };
            Some(B256::from(bytes))
        }
        _ => None,
    }
}

/// Convert a HandlerValue to a string key for tracking unique entries.
pub fn value_to_string(value: &HandlerValue) -> String {
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
