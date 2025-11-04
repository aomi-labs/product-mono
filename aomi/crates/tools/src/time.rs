use rig::{
    completion::ToolDefinition,
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Parameters for GetCurrentTime (empty struct - no parameters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCurrentTimeParameters {}

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
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let now = std::time::SystemTime::now();
        let duration = now
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ToolError::ToolCallError(format!("Time error: {}", e).into()))?;
        let seconds = duration.as_secs();

        Ok(seconds.to_string())
    }
}
