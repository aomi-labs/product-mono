//! Tools for requesting EIP-712 signatures from a connected wallet
use chrono::Utc;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// Parameters for RequestEip712Signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestEip712SignatureParameters {
    /// Short note shown to the user describing why they are signing
    pub topic: String,
    /// Full EIP-712 payload with `types`, `primaryType`, `domain`, and `message`
    pub typed_data: Value,
    /// Human-readable details shown alongside the wallet prompt
    pub description: String,
    /// Optional explicit request identifier (auto-generated when omitted)
    pub request_id: Option<String>,
    /// Optional ISO timestamp indicating when the signature expires
    pub expires_at: Option<String>,
    /// Optional expected signer address (0x-prefixed) for the frontend to verify
    pub expected_signer: Option<String>,
}

/// Tool marker type
#[derive(Debug, Clone)]
pub struct RequestEip712Signature;

fn ensure_object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, ToolError> {
    value.as_object().ok_or_else(|| {
        ToolError::ToolCallError(
            format!("'{field}' must be an object in EIP-712 typed data payload").into(),
        )
    })
}

fn validate_typed_data(typed_data: &Value) -> Result<(), ToolError> {
    let obj = typed_data
        .as_object()
        .ok_or_else(|| ToolError::ToolCallError("typed_data must be a JSON object".into()))?;

    for required_key in ["types", "primaryType", "domain", "message"] {
        if !obj.contains_key(required_key) {
            return Err(ToolError::ToolCallError(
                format!("typed_data missing required field '{required_key}'").into(),
            ));
        }
    }

    ensure_object(&obj["types"], "types")?;
    ensure_object(&obj["domain"], "domain")?;
    ensure_object(&obj["message"], "message")?;

    if !obj["primaryType"].is_string() {
        return Err(ToolError::ToolCallError(
            "primaryType must be a string".into(),
        ));
    }

    Ok(())
}

pub async fn execute_request(args: RequestEip712SignatureParameters) -> Result<Value, ToolError> {
    validate_typed_data(&args.typed_data)?;

    let RequestEip712SignatureParameters {
        topic,
        typed_data,
        description,
        request_id,
        expires_at,
        expected_signer,
    } = args;

    let request_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    Ok(json!({
        "topic": topic,
        "description": description,
        "request_id": request_id,
        "typed_data": typed_data,
        "expires_at": expires_at,
        "expected_signer": expected_signer,
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_typed_data() -> Value {
        json!({
            "types": {
                "EIP712Domain": [
                    {"name": "name", "type": "string"},
                    {"name": "chainId", "type": "uint256"}
                ],
                "Order": [
                    {"name": "maker", "type": "address"},
                    {"name": "market", "type": "bytes32"}
                ]
            },
            "domain": {
                "name": "Polymarket",
                "chainId": 137
            },
            "primaryType": "Order",
            "message": {
                "maker": "0x0000000000000000000000000000000000000000",
                "market": "0x01"
            }
        })
    }

    #[tokio::test]
    async fn test_execute_request_generates_id() {
        let payload = execute_request(RequestEip712SignatureParameters {
            topic: "Sign order".to_string(),
            typed_data: sample_typed_data(),
            description: "Sign limit order".to_string(),
            request_id: None,
            expires_at: None,
            expected_signer: None,
        })
        .await
        .unwrap();

        assert!(payload.get("request_id").is_some());
        assert_eq!(
            payload.get("topic").and_then(Value::as_str),
            Some("Sign order")
        );
    }

    #[tokio::test]
    async fn test_invalid_payload() {
        let err = execute_request(RequestEip712SignatureParameters {
            topic: "Bad".to_string(),
            typed_data: json!({"foo": "bar"}),
            description: "".to_string(),
            request_id: None,
            expires_at: None,
            expected_signer: None,
        })
        .await
        .unwrap_err();

        assert!(err.to_string().contains("missing required field"));
    }
}
