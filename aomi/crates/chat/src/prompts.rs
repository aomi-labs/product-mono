use std::{borrow::Cow, collections::HashMap};

use aomi_tools::clients::get_default_network_json;

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
    "Before reaching for web search or generic lookups, check whether an existing structured tool (GetContractABI, GetContractSourceCode, CallViewFunction, account/history tools, etc.) already provides the information you need. Prefer deterministic tools first; only search if the required data truly is not in-tool.",
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

pub fn agent_preamble_builder() -> PreambleBuilder {
    let cast_networks = match std::env::var("CHAIN_NETWORK_URLS_JSON") {
        Ok(json) => match serde_json::from_str::<HashMap<String, String>>(&json) {
            Ok(parsed) => parsed,
            Err(_) => get_default_network_json(),
        },
        Err(_) => get_default_network_json(),
    };
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

pub fn base_agent_preamble() -> String {
    agent_preamble_builder().build()
}

/// Creates formatted content for a conversation summary system message.
///
/// This function takes conversation summary details and formats them into a structured
/// message that instructs the LLM how to greet the user with historical context.
pub fn create_summary_content(
    marker: &str,
    main_topic: &str,
    key_details: &str,
    current_state: &str,
    user_friendly_summary: &str,
) -> String {
    let context_section = PromptSection::titled(marker.to_string())
        .paragraph(format!("Topic: {}", main_topic))
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

    #[test]
    fn test_base_agent_preamble() {
        let preamble = base_agent_preamble();
        println!("{}", preamble);
    }
}
