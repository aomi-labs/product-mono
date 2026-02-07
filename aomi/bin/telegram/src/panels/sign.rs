use anyhow::Result;
use serde_json::{json, Value};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};

pub struct SignPanel {
    base_url: String,
    message_template: &'static str,
}

impl SignPanel {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            message_template: "🔐 <b>Transaction requires your signature</b>\n\n\
                <b>To:</b> <code>{to}</code>\n\
                <b>Amount:</b> {amount}\n\
                <b>Description:</b> {description}\n\n\
                Tap the button below to review and sign.",
        }
    }

    pub fn message(&self, to: &str, amount: &str, description: &str) -> String {
        self.message_template
            .replace("{to}", to)
            .replace("{amount}", amount)
            .replace("{description}", description)
    }

    pub fn call_app(&self, tx_id: &str) -> InlineKeyboardMarkup {
        let url = format!("{}/sign?tx_id={}", self.base_url, tx_id);
        InlineKeyboardMarkup::new([[InlineKeyboardButton::web_app(
            "Sign Transaction",
            WebAppInfo {
                url: url.parse().unwrap(),
            },
        )]])
    }

    pub async fn create_pending_tx(&self, session_key: &str, tx: &Value) -> Result<String> {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/api/wallet/tx", self.base_url))
            .json(&json!({
                "session_key": session_key,
                "tx": {
                    "to": tx.get("to").and_then(|v| v.as_str()).unwrap_or(""),
                    "value": tx.get("value").and_then(|v| v.as_str()).unwrap_or("0"),
                    "data": tx.get("data").and_then(|v| v.as_str()).unwrap_or("0x"),
                    "chainId": tx.get("chainId").and_then(|v| v.as_u64()).unwrap_or(1),
                }
            }))
            .send()
            .await?;

        let result: Value = response.json().await?;
        let tx_id = result
            .get("txId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(tx_id)
    }
}
