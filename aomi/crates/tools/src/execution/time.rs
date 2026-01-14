use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for GetCurrentTime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentTimeParameters {
    /// One-line note on what this time check is for
    pub topic: String,
}

/// Tool for getting the current Unix timestamp
#[derive(Debug, Clone)]
pub struct GetCurrentTime;

// Original rig::Tool implementation (still used by existing code)
impl Tool for GetCurrentTime {
    const NAME: &'static str = "get_current_time";
    type Args = GetCurrentTimeParameters;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the current Unix timestamp in seconds".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": {
                        "type": "string",
                        "description": "Short note on what this time check is for"
                    }
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let _topic = args.topic;
        let now = std::time::SystemTime::now();
        let duration = now
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ToolError::ToolCallError(format!("Time error: {}", e).into()))?;
        let seconds = duration.as_secs();

        Ok(seconds.to_string())
    }
}

// V2: New AomiTool implementation
#[derive(Debug, Clone)]
pub struct GetCurrentTimeV2;

use aomi_tools_v2::AomiTool as AomiToolV2Trait;

impl AomiToolV2Trait for GetCurrentTimeV2 {
    const NAME: &'static str = "get_current_time";
    const NAMESPACE: &'static str = "time";

    type Args = GetCurrentTimeParameters;
    type Output = serde_json::Value;
    type Error = GetCurrentTimeError;

    fn support_async(&self) -> bool {
        false // This is a sync tool
    }

    fn description(&self) -> &'static str {
        "Get the current Unix timestamp in seconds"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Short note on what this time check is for"
                }
            },
            "required": ["topic"]
        })
    }

    async fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        args: Self::Args,
    ) {
        let _topic = args.topic;
        let result = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => {
                let seconds = duration.as_secs();
                Ok(json!(seconds.to_string()))
            }
            Err(e) => Err(eyre::eyre!("Time error: {}", e)),
        };
        let _ = result_sender.send(result);
    }
}

#[derive(Debug)]
pub struct GetCurrentTimeError(String);

impl std::fmt::Display for GetCurrentTimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetCurrentTimeError: {}", self.0)
    }
}

impl std::error::Error for GetCurrentTimeError {}
