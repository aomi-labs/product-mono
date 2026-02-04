use std::borrow::Cow;

use aomi_anvil::default_networks;
use tracing::{debug, warn};

// ============================================================================
// Account Context
// ============================================================================

/// Default anvil test accounts with their private keys
pub const ANVIL_ACCOUNTS: [(&str, &str); 10] = [
    // Account 0
    (
        "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
    ),
    // Account 1
    (
        "0x70997970C51812dc3A010C7d01b50e0d17dc79C8",
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    ),
    // Account 2
    (
        "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC",
        "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    ),
    // Account 3
    (
        "0x90F79bf6EB2c4f870365E785982E1f101E93b906",
        "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    ),
    // Account 4
    (
        "0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65",
        "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    ),
    // Account 5
    (
        "0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc",
        "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
    ),
    // Account 6
    (
        "0x976EA74026E726554dB657fA54763abd0C3a0aa9",
        "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
    ),
    // Account 7
    (
        "0x14dC79964da2C08b23698B3D3cc7Ca32193d9955",
        "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
    ),
    // Account 8
    (
        "0x23618e81E3f5cdF7f54C3d65f7FBc0aBf5B21E8f",
        "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
    ),
    // Account 9
    (
        "0xa0Ee7A142d267C1f36714E4a8F75612F20a79720",
        "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
    ),
];

pub fn generate_account_context() -> String {
    let account_count = ANVIL_ACCOUNTS.len();
    debug!(
        account_count = account_count,
        "Generating default Test account context for chat agent"
    );

    if account_count < 2 {
        warn!(
            account_count = account_count,
            "Only {account_count} default accounts available; context will be limited"
        );
    }

    let mut context = String::from("Available test accounts:\n");

    for (i, (address, _)) in ANVIL_ACCOUNTS.iter().take(2).enumerate() {
        let name = match i {
            0 => " (Alice)",
            1 => " (Bob)",
            _ => "",
        };
        debug!(
            index = i,
            address = %address,
            name = %name,
            "Adding default account to context"
        );
        context.push_str(&format!("- Account {i}: {address}{name}\n"));
    }

    context
        .push_str("\nYou can refer to these accounts by their names (Alice, Bob) or by their account numbers (0-9).");
    context.push_str("\n\nIMPORTANT: If the user has not connected a wallet, all transactions will be sent to the internal testnet (call with parameter \"testnet\" if needed). Remind the user to connect their wallet if they want to interact with mainnet or other networks.");
    debug!(final_length = context.len(), "Account context generated");
    context
}

// ============================================================================
// Agent Preamble
// ============================================================================

const AGENT_ROLE: &str = "You are an Ethereum ops assistant. Keep replies crisp, ground every claim in real tool output, and say \"I don't know\" or \"that failed\" whenever that is the truth.";

const WORKFLOW_STEPS: &[&str] = &[
    "Briefly name the step you're on.",
    "Parallelize tool calls as needed.",
    "Report what actually happened, including any failures.",
    "Repeat until the request is complete or blocked.",
];

const CONSTRAINTS: &[&str] = &[
    "Confirm whether each transaction succeeded and, when value moves, show the recipient balances that changed.",
    "Surface tool errors verbatim; never imply a failed call worked.",
    "During a single step you may run multiple tool calls, but only address the user between steps and never number the steps in your reply.",
    "When a transaction is rejected or cancelled by the user, acknowledge it and suggest alternatives or ask how they'd like to proceed.",
];

const TOOL_INSTRUCTIONS: &[&str] = &[
    "At the start of each session or when you need to know which network the user is on, call get_time_and_onchain_context to fetch the user's connected chain and current on-chain state. This tool automatically uses the user's wallet chain_id.",
    "Before reaching for web search or generic lookups, check whether an existing structured tool (GetContractABI, GetContractSourceCode, CallViewFunction, account/history tools, etc.) already provides the information you need. Prefer deterministic tools first; only search if the required data truly is not in-tool.",
    "Pay close attention to the tool descriptions and argument priority. When you have knowledge of optimal arguments, use them and don't treat the intent as an open ended request",
    "Before every send_transaction or send_transaction_to_wallet call, you MUST run simulate_contract_call with the same parameters. Always report full simulation details in your response, including success/failure, revert reason (if any), and the exact transaction data used in the simulation.",
    "If a simulation fails, adjust the transaction (e.g., calldata, value, params, gas assumptions) and retry the simulation up to 3 total attempts. Stop after 3 failed simulations, report each failure with revert details, and ask the user how to proceed. If the failure indicates insufficient funds or balance, do not auto-adjust the amount; ask the user for a revised amount before retrying.",
    "When reporting simulation results, include the raw simulate_contract_call JSON output (success/result/revert_reason/tx) in addition to any summary.",
    "When using testnet accounts (Alice/Bob), always pass the explicit hex address in tool calls rather than the name.",
];

const NETWORK_AWARENESS: &[&str] = &[
    "When a system message reports the user's wallet network (for example, \"User connected wallet … on ethereum\"), acknowledge it and use that exact network identifier in every tool call that requires a `network` argument. Do not prompt the user to switch networks—the UI already handles routing and simply keeps you informed.",
    "Example responses:\n- Got it, you're on ethereum. I'll run calls against that network.\n- Wallet disconnected, so I'll pause wallet-dependent actions until you reconnect.",
];

// TODO: Add examples
const EXAMPLES: &[&str] = &[];

const SUMMARY_INSTRUCTIONS: &[&str] = &[
    "Greet the user with the specific summary provided above",
    "Ask if they'd like to continue that conversation or start fresh",
    "If they want to start fresh (e.g., 'new conversation', 'start over', 'fresh start'), acknowledge it and don't reference the previous context anymore",
];

/// Represents a block of text in a prompt section.
#[derive(Clone, Debug)]
pub enum SectionBlock {
    Paragraph(Cow<'static, str>),
    OrderedList(Vec<Cow<'static, str>>),
    BulletList(Vec<Cow<'static, str>>),
    Blockquote(Vec<Cow<'static, str>>),
}

impl SectionBlock {
    fn render(&self) -> String {
        match self {
            SectionBlock::Paragraph(text) => text.trim().to_string(),
            SectionBlock::OrderedList(items) => items
                .iter()
                .enumerate()
                .map(|(idx, item)| format!("{}. {}", idx + 1, item.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
            SectionBlock::BulletList(items) => items
                .iter()
                .map(|item| format!("- {}", item.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
            SectionBlock::Blockquote(lines) => lines
                .iter()
                .map(|line| format!("> {}", line.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// A titled (or untitled) section of the system prompt.
#[derive(Clone, Debug, Default)]
pub struct PromptSection {
    heading: Option<Cow<'static, str>>,
    blocks: Vec<SectionBlock>,
}

impl PromptSection {
    pub fn titled<T>(title: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        Self {
            heading: Some(title.into()),
            blocks: Vec::new(),
        }
    }

    pub fn untitled() -> Self {
        Self {
            heading: None,
            blocks: Vec::new(),
        }
    }

    pub fn paragraph<T>(mut self, text: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.blocks.push(SectionBlock::Paragraph(text.into()));
        self
    }

    pub fn ordered_list<I, T>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Cow<'static, str>>,
    {
        self.blocks.push(SectionBlock::OrderedList(
            items.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn bullet_list<I, T>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Cow<'static, str>>,
    {
        self.blocks.push(SectionBlock::BulletList(
            items.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn blockquote<I, T>(mut self, lines: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Cow<'static, str>>,
    {
        self.blocks.push(SectionBlock::Blockquote(
            lines.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn block(mut self, block: SectionBlock) -> Self {
        self.blocks.push(block);
        self
    }

    fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(heading) = &self.heading {
            parts.push(format!("## {}", heading.trim()));
        }
        parts.extend(self.blocks.iter().map(SectionBlock::render));
        parts
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Builder for composing system prompts from reusable sections.
#[derive(Clone, Debug, Default)]
pub struct PreambleBuilder {
    sections: Vec<PromptSection>,
}

impl PreambleBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    pub fn section(mut self, section: PromptSection) -> Self {
        self.sections.push(section);
        self
    }

    pub fn extend<I>(mut self, sections: I) -> Self
    where
        I: IntoIterator<Item = PromptSection>,
    {
        self.sections.extend(sections);
        self
    }

    pub fn build(self) -> String {
        self.sections
            .into_iter()
            .map(|section| section.render())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub fn agent_identity_section() -> PromptSection {
    PromptSection::titled("Role").paragraph(AGENT_ROLE)
}

pub fn workflow_section() -> PromptSection {
    PromptSection::titled("Workflow").ordered_list(WORKFLOW_STEPS.iter().copied())
}

pub fn constraints_section() -> PromptSection {
    PromptSection::titled("Constraints").bullet_list(CONSTRAINTS.iter().copied())
}

pub fn tool_instructions_section() -> PromptSection {
    PromptSection::titled("Tool Instructions").bullet_list(TOOL_INSTRUCTIONS.iter().copied())
}

pub fn network_awareness_section() -> PromptSection {
    PromptSection::titled("Network Awareness").blockquote(NETWORK_AWARENESS.iter().copied())
}

pub fn examples_section() -> PromptSection {
    PromptSection::titled("Example responses").bullet_list(EXAMPLES.iter().copied())
}

pub async fn preamble_builder() -> PreambleBuilder {
    let cast_networks = default_networks().await.unwrap_or_default();
    let supported_networks = format!(
        "Supported networks: {}",
        cast_networks.keys().cloned().collect::<Vec<_>>().join(", ")
    );
    PreambleBuilder::new()
        .section(agent_identity_section())
        .section(workflow_section())
        .section(constraints_section())
        .section(tool_instructions_section())
        .section(network_awareness_section())
        .section(
            PromptSection::untitled()
                .paragraph(supported_networks)
                .paragraph("Reject requests to operate on unsupported networks."),
        )
}

pub async fn base_prompt() -> String {
    preamble_builder().await.build()
}

/// Creates formatted content for a conversation summary system message.
///
/// This function takes conversation summary details and formats them into a structured
/// message that instructs the LLM how to greet the user with historical context.
pub fn create_summary_content(
    marker: &str,
    title: &str,
    key_details: &str,
    current_state: &str,
    user_friendly_summary: &str,
) -> String {
    let context_section = PromptSection::titled(marker.to_string())
        .paragraph(format!("Topic: {}", title))
        .paragraph(format!("Details: {}", key_details))
        .paragraph(format!("Where they left off: {}", current_state))
        .paragraph(format!("Summary for user: {}", user_friendly_summary));

    let instructions_section =
        PromptSection::titled("Instructions").bullet_list(SUMMARY_INSTRUCTIONS.iter().copied());

    format!(
        "{}\n\n{}",
        context_section.render(),
        instructions_section.render()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_base_prompt() {
        let preamble = base_prompt().await;
        println!("{}", preamble);
    }
}
