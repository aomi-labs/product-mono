use crate::tools::{CreateQuote, FillQuote, GetQuote, GetReceipts, ListQuotes};
use aomi_core::{
    app::{AomiApp, CoreCommand, CoreCtx, CoreState},
    prompts::{PreambleBuilder, PromptSection},
    BuildOpts, CoreApp, CoreAppBuilder,
};
use async_trait::async_trait;
use eyre::Result;

// Type alias for DeltaRfqCommand with our specific ToolReturn type
pub type DeltaRfqCommand = CoreCommand;

/// Role configuration for the Delta RFQ agent
#[derive(Debug, Clone, Copy, Default)]
pub enum DeltaRole {
    /// Maker role: creates quotes and monitors fills
    Maker,
    /// Taker role: finds and fills quotes
    Taker,
    /// Both roles: can create and fill quotes
    #[default]
    Both,
}

// ============================================================================
// Preamble Content
// ============================================================================

const MAKER_ROLE: &str = "You are an AI assistant specialized in Delta RFQ Arena market making. You help makers create quotes using natural language, which are compiled into cryptographically-enforced 'Local Laws'. You monitor quote status and fill receipts to optimize trading strategies.";

const TAKER_ROLE: &str = "You are an AI assistant specialized in Delta RFQ Arena trade execution. You help takers find profitable quotes and execute fills with proper price feed evidence. You understand how Local Laws protect against invalid fills and ensure compliant execution.";

const BOTH_ROLE: &str = "You are an AI assistant specialized in Delta RFQ Arena trading. You can act as both a Maker (creating quotes with natural language constraints) and a Taker (executing fills with price feed evidence). You understand the cryptographic guarantees provided by Local Laws and ZK proofs.";

const DELTA_CONTEXT: &[&str] = &[
    "Delta RFQ Arena is an OTC Request-For-Quote trading system with cryptographic protections",
    "Makers post quotes in plain English (e.g., 'Buy 10 dETH at most 2000 USDD, expires 5 min')",
    "The backend compiles natural language into 'Local Laws' - machine-checkable guardrails",
    "Takers attempt to fill quotes by providing price feed evidence from multiple sources",
    "Only fills that satisfy Local Law constraints will settle - enforced via ZK proofs",
    "This eliminates counterparty risk: invalid fills are cryptographically impossible",
];

const LOCAL_LAW_EXPLANATION: &[&str] = &[
    "Local Laws are compiled constraints that protect makers from unfavorable fills",
    "They encode: asset type, direction (buy/sell), size limits, price bounds, expiration",
    "Example: 'Buy 10 dETH at most 2000 USDD' becomes a constraint checking fill_price <= 2000",
    "Multiple price feeds are required as evidence to prevent manipulation",
    "The ZK circuit verifies the fill satisfies ALL constraints before settlement",
];

const MAKER_CAPABILITIES: &[&str] = &[
    "Create quotes using natural language with automatic Local Law compilation",
    "Monitor quote status (active, filled, expired, cancelled)",
    "View fill receipts with ZK proofs of valid execution",
    "List all your active quotes in the arena",
];

const TAKER_CAPABILITIES: &[&str] = &[
    "Browse active quotes in the arena to find trading opportunities",
    "Execute fills with price feed evidence from multiple sources",
    "Verify fills satisfy Local Law constraints before submission",
    "View fill receipts and proofs for completed trades",
];

const MAKER_GUIDELINES: &[&str] = &[
    "Use clear, specific language when creating quotes (asset, direction, size, price limit, expiration)",
    "Example quote: 'I want to buy 10 dETH at most 2000 USDD each, expires in 5 minutes'",
    "Monitor your quotes regularly - expired quotes cannot be filled",
    "Review fill receipts to understand execution prices and verify ZK proofs",
];

const TAKER_GUIDELINES: &[&str] = &[
    "List quotes to find profitable opportunities matching your inventory",
    "Gather price feed evidence from multiple sources before attempting fills",
    "Ensure your fill price satisfies the quote's Local Law constraints",
    "Feed evidence must include: source, asset, price, timestamp, and signature",
    "If a fill fails, check that your price and evidence meet all constraints",
];

const SECURITY_NOTES: &[&str] = &[
    "Local Laws protect makers from price manipulation and stale data attacks",
    "Multiple price feeds prevent single-source manipulation",
    "ZK proofs ensure fills are verified without revealing sensitive strategy data",
    "Expired quotes automatically reject fills - time constraints are cryptographically enforced",
    "Invalid fills are mathematically impossible, not just economically discouraged",
];

fn maker_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(MAKER_ROLE))
        .section(
            PromptSection::titled("Understanding Delta RFQ Arena")
                .bullet_list(DELTA_CONTEXT.iter().copied()),
        )
        .section(
            PromptSection::titled("How Local Laws Work")
                .bullet_list(LOCAL_LAW_EXPLANATION.iter().copied()),
        )
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(MAKER_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(MAKER_GUIDELINES.iter().copied()),
        )
        .section(
            PromptSection::titled("Security Guarantees")
                .bullet_list(SECURITY_NOTES.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

fn taker_preamble() -> String {
    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(TAKER_ROLE))
        .section(
            PromptSection::titled("Understanding Delta RFQ Arena")
                .bullet_list(DELTA_CONTEXT.iter().copied()),
        )
        .section(
            PromptSection::titled("How Local Laws Work")
                .bullet_list(LOCAL_LAW_EXPLANATION.iter().copied()),
        )
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(TAKER_CAPABILITIES.iter().copied()),
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(TAKER_GUIDELINES.iter().copied()),
        )
        .section(
            PromptSection::titled("Security Guarantees")
                .bullet_list(SECURITY_NOTES.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

fn both_preamble() -> String {
    let all_capabilities: Vec<&str> = MAKER_CAPABILITIES
        .iter()
        .chain(TAKER_CAPABILITIES.iter())
        .copied()
        .collect();

    let all_guidelines: Vec<&str> = MAKER_GUIDELINES
        .iter()
        .chain(TAKER_GUIDELINES.iter())
        .copied()
        .collect();

    PreambleBuilder::new()
        .section(PromptSection::titled("Role").paragraph(BOTH_ROLE))
        .section(
            PromptSection::titled("Understanding Delta RFQ Arena")
                .bullet_list(DELTA_CONTEXT.iter().copied()),
        )
        .section(
            PromptSection::titled("How Local Laws Work")
                .bullet_list(LOCAL_LAW_EXPLANATION.iter().copied()),
        )
        .section(
            PromptSection::titled("Your Capabilities")
                .bullet_list(all_capabilities.iter().copied()),
        )
        .section(
            PromptSection::titled("Execution Guidelines")
                .bullet_list(all_guidelines.iter().copied()),
        )
        .section(
            PromptSection::titled("Security Guarantees")
                .bullet_list(SECURITY_NOTES.iter().copied()),
        )
        .section(
            PromptSection::titled("Account Context")
                .paragraph(aomi_core::generate_account_context()),
        )
        .build()
}

// ============================================================================
// App Implementation
// ============================================================================

pub struct DeltaRfqApp {
    chat_app: CoreApp,
    role: DeltaRole,
}

impl DeltaRfqApp {
    pub async fn default() -> Result<Self> {
        Self::new(BuildOpts::default(), DeltaRole::Both).await
    }

    pub async fn maker() -> Result<Self> {
        Self::new(BuildOpts::default(), DeltaRole::Maker).await
    }

    pub async fn taker() -> Result<Self> {
        Self::new(BuildOpts::default(), DeltaRole::Taker).await
    }

    pub async fn new(opts: BuildOpts, role: DeltaRole) -> Result<Self> {
        let preamble = match role {
            DeltaRole::Maker => maker_preamble(),
            DeltaRole::Taker => taker_preamble(),
            DeltaRole::Both => both_preamble(),
        };

        let mut builder = CoreAppBuilder::new(&preamble, opts, None).await?;

        // Add tools based on role
        match role {
            DeltaRole::Maker => {
                builder.add_tool(CreateQuote)?;
                builder.add_tool(ListQuotes)?;
                builder.add_tool(GetQuote)?;
                builder.add_tool(GetReceipts)?;
            }
            DeltaRole::Taker => {
                builder.add_tool(ListQuotes)?;
                builder.add_tool(GetQuote)?;
                builder.add_tool(FillQuote)?;
                builder.add_tool(GetReceipts)?;
            }
            DeltaRole::Both => {
                builder.add_tool(CreateQuote)?;
                builder.add_tool(ListQuotes)?;
                builder.add_tool(GetQuote)?;
                builder.add_tool(FillQuote)?;
                builder.add_tool(GetReceipts)?;
            }
        }

        // Build the final DeltaRfqApp
        let chat_app = builder.build(opts, None).await?;

        Ok(Self { chat_app, role })
    }

    pub fn role(&self) -> DeltaRole {
        self.role
    }

    pub async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        tracing::debug!(
            "[delta-rfq] process message (role={:?}): {}",
            self.role,
            input
        );
        self.chat_app.process_message(input, state, ctx).await
    }
}

#[async_trait]
impl AomiApp for DeltaRfqApp {
    type Command = CoreCommand;

    async fn process_message(
        &self,
        input: String,
        state: &mut CoreState,
        ctx: CoreCtx<'_>,
    ) -> Result<()> {
        DeltaRfqApp::process_message(self, input, state, ctx).await
    }

    fn tool_namespaces(&self) -> std::sync::Arc<std::collections::HashMap<String, String>> {
        self.chat_app.tool_namespaces()
    }
}
