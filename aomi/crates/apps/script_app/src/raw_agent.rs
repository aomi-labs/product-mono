//! Text editor tools with mock filesystem, following Claude's text_editor schema.

use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{CompletionModel, ToolDefinition},
    tool::{Tool, ToolError},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

pub type MockFs = Arc<RwLock<HashMap<String, String>>>;

pub fn create_mock_fs() -> MockFs {
    Arc::new(RwLock::new(HashMap::new()))
}

// ============================================================================
// ViewFile
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewFileArgs {
    pub path: String,
    #[serde(default)]
    pub view_range: Option<[i64; 2]>,
}

#[derive(Clone)]
pub struct ViewFile(pub MockFs);

impl Tool for ViewFile {
    const NAME: &'static str = "view";
    type Args = ViewFileArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "View file contents with line numbers. Use view_range [start, end] for partial view (1-indexed, -1 = EOF).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" },
                    "view_range": { "type": "array", "items": { "type": "integer" }, "description": "[start, end] 1-indexed, -1 = EOF" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let fs = self.0.read().await;
        let content = fs.get(&args.path).cloned().ok_or_else(|| {
            ToolError::ToolCallError(format!("Error: File not found: {}", args.path).into())
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let (start, end) = match args.view_range {
            Some([s, e]) => (
                (s.max(1) - 1) as usize,
                if e == -1 {
                    lines.len()
                } else {
                    (e as usize).min(lines.len())
                },
            ),
            None => (0, lines.len()),
        };

        Ok(lines
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{}: {}", start + i + 1, l))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

// ============================================================================
// StrReplace
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrReplaceArgs {
    pub path: String,
    pub old_str: String,
    pub new_str: String,
}

#[derive(Clone)]
pub struct StrReplace(pub MockFs);

impl Tool for StrReplace {
    const NAME: &'static str = "str_replace";
    type Args = StrReplaceArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Replace exact text in file. Must match exactly once.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" },
                    "old_str": { "type": "string", "description": "Text to replace (exact match)" },
                    "new_str": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_str", "new_str"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut fs = self.0.write().await;
        let content = fs.get(&args.path).cloned().ok_or_else(|| {
            ToolError::ToolCallError(format!("Error: File not found: {}", args.path).into())
        })?;

        match content.matches(&args.old_str).count() {
            0 => Err(ToolError::ToolCallError("Error: No match found".into())),
            1 => {
                let updated = content.replacen(&args.old_str, &args.new_str, 1);
                fs.insert(args.path, updated);
                Ok("Successfully replaced text.".to_string())
            }
            n => Err(ToolError::ToolCallError(
                format!("Error: Found {} matches, need exactly 1", n).into(),
            )),
        }
    }
}

// ============================================================================
// Insert
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertArgs {
    pub path: String,
    pub insert_line: u32,
    pub new_str: String,
}

#[derive(Clone)]
pub struct Insert(pub MockFs);

impl Tool for Insert {
    const NAME: &'static str = "insert";
    type Args = InsertArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Insert text after specified line (0 = beginning).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" },
                    "insert_line": { "type": "integer", "description": "Line to insert after (0 = start)" },
                    "new_str": { "type": "string", "description": "Text to insert" }
                },
                "required": ["path", "insert_line", "new_str"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut fs = self.0.write().await;
        let content = fs.get(&args.path).cloned().ok_or_else(|| {
            ToolError::ToolCallError(format!("Error: File not found: {}", args.path).into())
        })?;

        let mut lines: Vec<&str> = content.lines().collect();
        let idx = (args.insert_line as usize).min(lines.len());
        for (i, line) in args.new_str.lines().enumerate() {
            lines.insert(idx + i, line);
        }
        fs.insert(args.path, lines.join("\n"));
        Ok(format!("Inserted after line {}", args.insert_line))
    }
}

// ============================================================================
// Agent Builder
// ============================================================================

// pub fn build_editor_agent<M>(client: &M::Client, model: impl Into<String>, fs: MockFs) -> Agent<M>
// where
//     M: CompletionModel,
//     M::Client: CompletionClient<M>,
// {
//     client
//         .agent(model)
//         .preamble("You are a code editor. Use view, str_replace, and insert tools to edit files.")
//         .tool(ViewFile(fs.clone()))
//         .tool(StrReplace(fs.clone()))
//         .tool(Insert(fs))
//         .build()
// }
