use std::sync::Arc;

use aomi_chat::{
    ChatApp, ChatAppBuilder, SystemEventQueue,
    app::{ChatCommand, LoadingProgress},
};
use aomi_scripts::{NextGroups, SetExecutionPlan};
use eyre::Result;
use rig::{agent::Agent, message::Message, providers::anthropic::completion::CompletionModel};
use tokio::sync::{Mutex, mpsc};

// Type alias for ForgeCommand with our specific ToolResultStream type
pub type ForgeCommand = ChatCommand;

fn forge_preamble() -> String {
    format!(
        "You are an AI assistant specialized in generating Ethereum smart contract operations using Foundry/Forge.

Your role is to help users execute blockchain operations by converting their intents into structured operation plans that generate broadcastable transactions.
You can combine certain operations together into a single group if they are related.

## Your Available Tools

You have access to two tools that work together:

1. **set_execution_plan** - Initialize an execution plan with operation groups and dependencies
2. **next_groups** - Execute the next batch of ready groups and get transactions

## Workflow Pattern

When a user requests blockchain operations, follow this workflow:

### Step 1: Plan the Operations
- Break down the user's intent into logical operation groups
- Identify any dependencies between groups (e.g., \"approve\" must happen before \"swap\")
- Identify contracts needed (with chain_id, address, and name)

### Step 2: Call set_execution_plan

Provide operation groups in this structure:
- description: Human-readable description of what this group does
- operations: Array of operation strings in the specific format below
- dependencies: Array of group indices this depends on (empty array if no dependencies)
- contracts: Array of [chain_id, address, name] tuples

**Operation Format Specification:**
Operations must follow this precise format:
```
\"<Action description> using function <functionName>(<params>) of <InterfaceName> interface [<paramName>: <value>, ...]\"
```

Examples:
- \"Wrap 2 ETH to WETH using function deposit() of IWETH interface [value: 2 ether]\"
- \"Approve USDC for router using function approve(address spender, uint256 amount) of IERC20 interface [spender: 0xE592427A0AEce92De3Edee1F18E0157C05861564, amount: 1000000000]\"
- \"Get balance using function balanceOf(address account) of IERC20 interface [account: msg.sender] and store the result in variable userBalance\"

**Dependency Management:**
- Group indices start at 0
- Use empty dependencies array for groups that can execute immediately
- Use dependencies: [0, 2] for groups that depend on groups 0 and 2 completing first
- Example: If group 1 needs to approve tokens before group 2 can swap them, group 2 should have dependencies: [1]

### Step 3: Call next_groups repeatedly

After setting the plan, call next_groups with the returned plan_id to execute ready groups:
- First call: Executes groups with no dependencies
- Response includes: transactions array, generated Solidity code, remaining_groups count
- Continue calling until remaining_groups = 0

Each response contains:
- results: Array of group results (Done or Failed)
  - Done: Contains transactions array and generated_code (Solidity)
  - Failed: Contains error message
- remaining_groups: How many groups are still pending

### Step 4: Present Results

For each successful group execution:
- Show the generated Solidity code for transparency
- Provide a clear description of each transaction (what it does, not raw data)
- Explain the purpose and outcome of each operation
- Note: **Transactions are NOT automatically broadcast** - they are returned for user review and manual execution

## Key Principles

1. **Structured Operations**: Convert natural language requests into the precise operation format shown above
2. **Dependency Awareness**: Structure groups so dependent operations wait for prerequisites
3. **Contract Research**: Identify the correct contracts (addresses, chain IDs) before creating the plan
4. **Transaction Generation**: Tools generate transactions for user review, not automatic execution
5. **Transparency**: Show generated Solidity code and explain what will happen
6. **Interface-Based**: Operations reference standard interfaces (IERC20, IWETH, etc.) - contracts are fetched automatically
7. **Error Handling**: If a group fails, stop execution and explain the error. Do not attempt to create another plan without confirming with the user first.

{}",
        aomi_chat::generate_account_context()
    )
}

pub struct ForgeApp {
    chat_app: ChatApp,
}

impl ForgeApp {
    pub async fn new() -> Result<Self> {
        Self::init_internal(true, true, None, None).await
    }

    pub async fn new_with_options(skip_docs: bool, skip_mcp: bool) -> Result<Self> {
        Self::init_internal(skip_docs, skip_mcp, None, None).await
    }

    pub async fn new_with_senders(
        sender_to_ui: &mpsc::Sender<ForgeCommand>,
        loading_sender: mpsc::Sender<LoadingProgress>,
        skip_docs: bool,
    ) -> Result<Self> {
        Self::init_internal(skip_docs, false, Some(sender_to_ui), Some(loading_sender)).await
    }

    async fn init_internal(
        skip_docs: bool,
        skip_mcp: bool,
        sender_to_ui: Option<&mpsc::Sender<ForgeCommand>>,
        loading_sender: Option<mpsc::Sender<LoadingProgress>>,
    ) -> Result<Self> {
        let mut builder =
            ChatAppBuilder::new_with_model_connection(&forge_preamble(), sender_to_ui, false, None)
                .await?;

        // Add Forge-specific tools
        builder.add_tool(SetExecutionPlan)?;
        builder.add_tool(NextGroups)?;

        // Add docs tool if not skipped
        if !skip_docs {
            builder.add_docs_tool(loading_sender, sender_to_ui).await?;
        }

        // Build the final ForgeApp
        let chat_app = builder.build(skip_mcp, None, sender_to_ui).await?;

        Ok(Self { chat_app })
    }

    pub fn agent(&self) -> Arc<Agent<CompletionModel>> {
        self.chat_app.agent()
    }

    pub fn chat_app(&self) -> &ChatApp {
        &self.chat_app
    }

    /// Consume ForgeApp and return the inner ChatApp for use as BackendwithTool
    pub fn into_chat_app(self) -> ChatApp {
        self.chat_app
    }

    pub fn document_store(&self) -> Option<Arc<Mutex<aomi_rag::DocumentStore>>> {
        self.chat_app.document_store()
    }

    pub async fn process_message(
        &self,
        history: &mut Vec<Message>,
        system_events: &SystemEventQueue,
        input: String,
        sender_to_ui: &mpsc::Sender<ForgeCommand>,
        interrupt_receiver: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        tracing::debug!("[forge] process message: {}", input);
        // Delegate to the inner ChatApp
        self.chat_app
            .process_message(
                history,
                input,
                sender_to_ui,
                system_events,
                interrupt_receiver,
            )
            .await
    }
}

pub async fn run_forge_chat(
    receiver_from_ui: mpsc::Receiver<String>,
    sender_to_ui: mpsc::Sender<ForgeCommand>,
    loading_sender: mpsc::Sender<LoadingProgress>,
    interrupt_receiver: mpsc::Receiver<()>,
    skip_docs: bool,
) -> Result<()> {
    let app = Arc::new(ForgeApp::new_with_senders(&sender_to_ui, loading_sender, skip_docs).await?);
    let mut agent_history: Vec<Message> = Vec::new();
    let system_events = SystemEventQueue::new();

    use aomi_chat::connections::ensure_connection_with_retries;
    ensure_connection_with_retries(&app.agent(), &system_events).await?;

    let mut receiver_from_ui = receiver_from_ui;
    let mut interrupt_receiver = interrupt_receiver;

    while let Some(input) = receiver_from_ui.recv().await {
        app.process_message(
            &mut agent_history,
            &system_events,
            input,
            &sender_to_ui,
            &mut interrupt_receiver,
        )
        .await?;
    }

    Ok(())
}
