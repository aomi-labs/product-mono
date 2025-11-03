use rig_derive::rig_tool;
use tracing::info;

#[rig_tool]
pub fn get_current_time() -> Result<String, rig::tool::ToolError> {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
    let seconds = duration.as_secs();

    info!(
        target: "aomi_tools::time",
        unix_seconds = seconds,
        "Returning current unix timestamp"
    );

    Ok(seconds.to_string())
}

impl_rig_tool_clone!(GetCurrentTime, GetCurrentTimeParameters, []);
