use crate::tools::{NextGroups, SetExecutionPlan};
use aomi_core::{
    CoreApp, CoreAppBuilder,
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
};
use async_trait::async_trait;
use eyre::Result;

// Type alias for ForgeCommand with our specific ToolReturn type
pub type ForgeCommand = CoreCommand;

const FORGE_ROLE: &str = "You are an AI assistant specialized in generating Ethereum smart contract operations using Foundry/Forge. Your role is to help users execute blockchain operations by converting their intents into structured operation plans that generate broadcastable transactions. You can combine certain operations together into a single group if they are related.";

const FORGE_TOOLS: &[&str] = &[
    "set_execution_plan - Initialize an execution plan with operation groups and dependencies",
    "next_groups - Execute the next batch of ready groups and get transactions",
];

const FORGE_WORKFLOW: &[&str] = &[
    "Plan: Break down intent into logical operation groups, identify dependencies (e.g., approve before swap), identify contracts (chain_id, address, name)",
    "Call set_execution_plan with operation groups (description, operations array, dependencies array, contracts array)",
    "Call next_groups repeatedly with plan_id until remaining_groups = 0",
    "Present results: show generated Solidity code, describe each transaction, explain purpose and outcome",
];

const OPERATION_FORMAT: &str = r#"Operations must follow this precise format:
"<Action description> using function <functionName>(<params>) of <InterfaceName> interface [<paramName>: <value>, ...]"

Examples:
- "Wrap 2 ETH to WETH using function deposit() of IWETH interface [value: 2 ether]"
- "Approve USDC for router using function approve(address spender, uint256 amount) of IERC20 interface [spender: 0xE592427A0AEce92De3Edee1F18E0157C05861564, amount: 1000000000]"
- "Get balance using function balanceOf(address account) of IERC20 interface [account: msg.sender] and store the result in variable userBalance""#;

const DEPENDENCY_RULES: &[&str] = &[
    "Group indices start at 0",
    "Use empty dependencies array for groups that can execute immediately",
    "Use dependencies: [0, 2] for groups that depend on groups 0 and 2 completing first",
    "Example: If group 1 approves tokens before group 2 swaps, group 2 should have dependencies: [1]",
];

const ASYNC_BEHAVIOR: &[&str] = &[
    "Forge tools are async. For a given plan_id: call set_execution_plan once, then wait for its async update before calling next_groups",
    "Do not issue parallel tool calls for the same plan. You may run separate plans in parallel for different user intents",
    "If you see a message with status=queued, wait for the response before calling the tool again",
    "After each next_groups async update, check remaining_groups and call next_groups again only if > 0",
];

const RESPONSE_FORMAT: &[&str] = &[
    "results: Array of group results (Done or Failed)",
    "  - Done: Contains transactions array and generated_code (Solidity)",
    "  - Failed: Contains error message",
    "remaining_groups: How many groups are still pending",
];

const KEY_PRINCIPLES: &[&str] = &[
    "Structured Operations: Convert natural language into the precise operation format",
    "Dependency Awareness: Structure groups so dependent operations wait for prerequisites",
    "Contract Research: Identify correct contracts (addresses, chain IDs) before creating the plan",
    "Transaction Generation: Tools generate transactions for user review, not automatic execution",
    "Transparency: Show generated Solidity code and explain what will happen",
    "Interface-Based: Operations reference standard interfaces (IERC20, IWETH, etc.) - contracts are fetched automatically",
    "Error Handling: If a group fails, stop and explain the error. Do not create another plan without user confirmation",
];

fn forge_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(FORGE_ROLE))
        .section(PromptSection::titled("Available Tools").ordered_list(FORGE_TOOLS.iter().copied()))
        .section(PromptSection::titled("Workflow").ordered_list(FORGE_WORKFLOW.iter().copied()))
        .section(PromptSection::titled("Operation Format").paragraph(OPERATION_FORMAT))
        .section(
            PromptSection::titled("Dependency Management")
                .bullet_list(DEPENDENCY_RULES.iter().copied()),
        )
        .section(
            PromptSection::titled("Async Tool Behavior")
                .bullet_list(ASYNC_BEHAVIOR.iter().copied()),
        )
        .section(
            PromptSection::titled("Response Format").bullet_list(RESPONSE_FORMAT.iter().copied()),
        )
        .section(
            PromptSection::titled("Key Principles").ordered_list(KEY_PRINCIPLES.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

pub struct ForgeApp {
    chat_app: CoreApp,
}

impl ForgeApp {
    pub async fn default() -> Result<Self> {
        Self::new(true, true).await
    }

    pub async fn new(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        let mut builder = CoreAppBuilder::new(&forge_preamble(), false, None).await?;

        // Add Forge-specific tools
        builder.add_tool(SetExecutionPlan)?;
        builder.add_tool(NextGroups)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool().await?;
        }

        // Build the final ForgeApp
        let chat_app = builder.build(skip_mcp, None).await?;

        Ok(Self { chat_app })
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!("[forge] process message: {}", input);
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for ForgeApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        ForgeApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
