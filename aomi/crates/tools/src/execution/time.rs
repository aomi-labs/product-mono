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

use crate::AomiTool;

impl AomiTool for GetCurrentTime {
    const NAME: &'static str = "get_current_time";

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

    fn run_sync(
        &self,
        result_sender: tokio::sync::oneshot::Sender<eyre::Result<serde_json::Value>>,
        args: Self::Args,
    ) -> impl std::future::Future<Output = ()> + Send {
        async move {
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
}

#[derive(Debug)]
pub struct GetCurrentTimeError(String);

impl std::fmt::Display for GetCurrentTimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GetCurrentTimeError: {}", self.0)
    }
}

impl std::error::Error for GetCurrentTimeError {}
