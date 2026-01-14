pub mod args;
pub mod tool_trait;
pub mod types;
pub mod wrapper;

pub use args::AomiToolArgs;
pub use tool_trait::AomiTool;
pub use types::{CallMetadata, ToolMetadata};
pub use wrapper::AomiToolWrapper;

// Macro helper for wrapping tools
#[macro_export]
macro_rules! aomi_tool {
    ($tool:expr) => {
        $crate::AomiToolWrapper::new($tool)
    };
}
