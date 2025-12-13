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
