#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AomiModel {
    ClaudeSonnet4,
    ClaudeOpus4,
    ClaudeHaiku35,
    Gpt5,
    Gpt5Mini,
    Gpt5Chat,
    Fast,
    OpenaiFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub rig: AomiModel,
    pub baml: AomiModel,
}

impl Default for Selection {
    fn default() -> Self {
        Self { rig: AomiModel::ClaudeOpus4, baml: AomiModel::ClaudeOpus4 }
    }
}

impl AomiModel {
    pub const fn rig_provider(self) -> Option<&'static str> {
        match self {
            AomiModel::ClaudeSonnet4 | AomiModel::ClaudeOpus4 | AomiModel::ClaudeHaiku35 => {
                Some("anthropic")
            }
            AomiModel::Gpt5 | AomiModel::Gpt5Mini | AomiModel::Gpt5Chat => Some("openai"),
            AomiModel::Fast | AomiModel::OpenaiFallback => None,
        }
    }

    pub const fn rig_id(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "claude-sonnet-4-20250514",
            AomiModel::ClaudeOpus4 => "claude-opus-4-1-20250805",
            AomiModel::ClaudeHaiku35 => "claude-3-5-haiku-20241022",
            AomiModel::Gpt5 => "gpt-5",
            AomiModel::Gpt5Mini => "gpt-5-mini",
            AomiModel::Gpt5Chat => "gpt-5",
            AomiModel::Fast => "gpt-5-mini",
            AomiModel::OpenaiFallback => "gpt-5",
        }
    }

    pub const fn rig_slug(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "sonnet-4",
            AomiModel::ClaudeOpus4 => "opus-4",
            AomiModel::ClaudeHaiku35 => "haiku-3-5",
            AomiModel::Gpt5 => "gpt-5",
            AomiModel::Gpt5Mini => "gpt-5-mini",
            AomiModel::Gpt5Chat => "gpt-5-chat",
            AomiModel::Fast => "fast",
            AomiModel::OpenaiFallback => "openai-fallback",
        }
    }

    pub const fn rig_label(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "Claude Sonnet 4",
            AomiModel::ClaudeOpus4 => "Claude Opus 4.1",
            AomiModel::ClaudeHaiku35 => "Claude 3.5 Haiku",
            AomiModel::Gpt5 => "OpenAI GPT-5 (Responses)",
            AomiModel::Gpt5Mini => "OpenAI GPT-5 Mini (Responses)",
            AomiModel::Gpt5Chat => "OpenAI GPT-5 (Chat)",
            AomiModel::Fast => "Fast Round Robin",
            AomiModel::OpenaiFallback => "OpenAI Fallback",
        }
    }

    pub const fn baml_client_name(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "CustomSonnet4",
            AomiModel::ClaudeOpus4 => "CustomOpus4",
            AomiModel::ClaudeHaiku35 => "CustomHaiku",
            AomiModel::Gpt5 => "CustomGPT5",
            AomiModel::Gpt5Mini => "CustomGPT5Mini",
            AomiModel::Gpt5Chat => "CustomGPT5Chat",
            AomiModel::Fast => "CustomFast",
            AomiModel::OpenaiFallback => "OpenaiFallback",
        }
    }

    pub const fn baml_label(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "Claude Sonnet 4",
            AomiModel::ClaudeOpus4 => "Claude Opus 4.1",
            AomiModel::ClaudeHaiku35 => "Claude 3.5 Haiku",
            AomiModel::Gpt5 => "OpenAI GPT-5",
            AomiModel::Gpt5Mini => "OpenAI GPT-5 Mini",
            AomiModel::Gpt5Chat => "OpenAI GPT-5 Chat",
            AomiModel::Fast => "Custom Fast (Round Robin)",
            AomiModel::OpenaiFallback => "OpenAI Fallback",
        }
    }

    pub fn parse_rig(input: &str) -> Option<Self> {
        let normalized = input.trim().to_lowercase();
        match normalized.as_str() {
            "sonnet" | "sonnet-4" | "claude-sonnet-4-20250514" => Some(AomiModel::ClaudeSonnet4),
            "opus" | "opus-4" | "opus-4.1" | "claude-opus-4-1-20250805" => {
                Some(AomiModel::ClaudeOpus4)
            }
            "haiku" | "haiku-3-5" | "claude-3-5-haiku-20241022" => Some(AomiModel::ClaudeHaiku35),
            "gpt-5" | "gpt5" | "openai-gpt-5" | "openai-gpt5" => Some(AomiModel::Gpt5),
            "gpt-5-mini" | "gpt5-mini" | "openai-gpt-5-mini" | "openai-gpt5-mini" => {
                Some(AomiModel::Gpt5Mini)
            }
            "gpt-5-chat" | "gpt5-chat" | "openai-gpt-5-chat" | "openai-gpt5-chat" => {
                Some(AomiModel::Gpt5Chat)
            }
            _ => None,
        }
    }

    pub fn parse_baml(input: &str) -> Option<Self> {
        let normalized = input.trim().to_lowercase();
        match normalized.as_str() {
            "opus" | "opus-4" | "opus-4.1" | "defaultopus4" | "customopus4" => {
                Some(AomiModel::ClaudeOpus4)
            }
            "sonnet" | "sonnet-4" | "customsonnet4" => Some(AomiModel::ClaudeSonnet4),
            "haiku" | "haiku-3-5" | "customhaiku" => Some(AomiModel::ClaudeHaiku35),
            "gpt-5" | "gpt5" | "customgpt5" => Some(AomiModel::Gpt5),
            "gpt-5-mini" | "gpt5-mini" | "customgpt5mini" => Some(AomiModel::Gpt5Mini),
            "gpt-5-chat" | "gpt5-chat" | "customgpt5chat" => Some(AomiModel::Gpt5Chat),
            "fast" | "customfast" | "round-robin" | "roundrobin" => Some(AomiModel::Fast),
            "openai-fallback" | "openai_fallback" | "fallback" | "openaifallback" => {
                Some(AomiModel::OpenaiFallback)
            }
            _ => None,
        }
    }

    pub const fn rig_all() -> &'static [AomiModel] {
        &[
            AomiModel::ClaudeSonnet4,
            AomiModel::ClaudeOpus4,
            AomiModel::ClaudeHaiku35,
            AomiModel::Gpt5,
            AomiModel::Gpt5Mini,
            AomiModel::Gpt5Chat,
        ]
    }

    pub const fn baml_all() -> &'static [AomiModel] {
        &[
            AomiModel::ClaudeOpus4,
            AomiModel::ClaudeSonnet4,
            AomiModel::ClaudeHaiku35,
            AomiModel::Gpt5,
            AomiModel::Gpt5Mini,
            AomiModel::Gpt5Chat,
            AomiModel::Fast,
            AomiModel::OpenaiFallback,
        ]
    }
}
