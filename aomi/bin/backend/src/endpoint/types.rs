//! API response types for the backend endpoints.

use aomi_backend::ChatMessage;
use serde::Serialize;

/// Response wrapper for system messages
#[derive(Serialize)]
pub struct SystemResponse {
    pub res: ChatMessage,
}
