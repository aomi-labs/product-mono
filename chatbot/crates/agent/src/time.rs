use rig_derive::rig_tool;

#[rig_tool]
pub fn get_current_time() -> Result<String, rig::tool::ToolError> {
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap();
    let seconds = duration.as_secs();

    Ok(seconds.to_string())
}
