use aomi_chat::{
    CoreApp, CoreAppBuilder,
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
};
use async_trait::async_trait;
use eyre::Result;

use crate::tools::{CompileSession, EditScript, ExecuteContract, FetchContract, SearchDocs};

pub type ScriptAppCommand = CoreCommand;

fn script_app_preamble() -> String {
    format!(
        "You are an AI assistant that reviews and iterates on generated Solidity scripts.

Your workflow:
- Review the generated script and dependencies
- Edit and retry compilation when needed
- Execute the script and audit results for correctness

Tools will provide compile, execute, edit, fetch, and doc search capabilities.\n\n{}",
        aomi_chat::generate_account_context()
    )
}

pub struct ScriptApp {
    chat_app: CoreApp,
}

impl ScriptApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true).await
    }

    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&script_app_preamble(), false, None).await?;

        builder.add_tool(CompileSession)?;
        builder.add_tool(ExecuteContract)?;
        builder.add_tool(EditScript)?;
        builder.add_tool(FetchContract)?;
        builder.add_tool(SearchDocs)?;

        if !skip_docs {
            builder.add_docs_tool().await?;
        }

        let chat_app = builder.build(skip_mcp, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[script_app] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for ScriptApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        ScriptApp::process_message(self, input, state, ctx).await
    }
}
