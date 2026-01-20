use aomi_core::{
    CoreApp, CoreAppBuilder,
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
};
use async_trait::async_trait;
use eyre::Result;

use crate::tools::{
    AdminCreateApiKey, AdminDeleteContract, AdminDeleteSession, AdminDeleteUser, AdminListApiKeys,
    AdminListContracts, AdminListSessions, AdminListUsers, AdminUpdateApiKey, AdminUpdateContract,
    AdminUpdateSession, AdminUpdateUser,
};

pub type AdminCommand = CoreCommand;

const ADMIN_ROLE: &str = "You are an AI assistant for admin database operations. Use the admin tools to list, create, update, and delete API keys, users, sessions, and contracts. Always confirm destructive actions (deletes or clearing fields) before executing.";

const ADMIN_TOOLS: &[&str] = &[
    "admin_create_api_key - Create an API key with namespaces",
    "admin_list_api_keys - List API keys (with optional filters)",
    "admin_update_api_key - Update label/namespaces/active status for an API key",
    "admin_list_users - List users",
    "admin_update_user - Update a user's username",
    "admin_delete_user - Delete a user",
    "admin_list_sessions - List sessions",
    "admin_update_session - Update session title or public key",
    "admin_delete_session - Delete a session",
    "admin_list_contracts - List contracts with filters",
    "admin_update_contract - Update contract metadata",
    "admin_delete_contract - Delete a contract",
];

const ADMIN_WORKFLOW: &[&str] = &[
    "Clarify intent and any filters or identifiers needed",
    "Use the appropriate admin_* tool for the operation",
    "Return concise JSON results and confirm any risky changes",
];

fn admin_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(ADMIN_ROLE))
        .section(PromptSection::titled("Tools").bullet_list(ADMIN_TOOLS.iter().copied()))
        .section(PromptSection::titled("Workflow").ordered_list(ADMIN_WORKFLOW.iter().copied()))
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

pub struct AdminApp {
    chat_app: CoreApp,
}

impl AdminApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true).await
    }

    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&admin_preamble(), false, None).await?;

        builder.add_tool(AdminCreateApiKey)?;
        builder.add_tool(AdminListApiKeys)?;
        builder.add_tool(AdminUpdateApiKey)?;
        builder.add_tool(AdminListUsers)?;
        builder.add_tool(AdminUpdateUser)?;
        builder.add_tool(AdminDeleteUser)?;
        builder.add_tool(AdminListSessions)?;
        builder.add_tool(AdminUpdateSession)?;
        builder.add_tool(AdminDeleteSession)?;
        builder.add_tool(AdminListContracts)?;
        builder.add_tool(AdminUpdateContract)?;
        builder.add_tool(AdminDeleteContract)?;

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
        tracing::debug!("[admin] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for AdminApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        AdminApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
