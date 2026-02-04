/// Model provider for routing
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ModelProvider {
    Anthropic,
    OpenAI,
    OpenRouter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AomiModel {
    // Anthropic models (direct)
    ClaudeOpus45,
    ClaudeSonnet45,
    ClaudeHaiku45,
    ClaudeOpus4,
    ClaudeSonnet4,
    ClaudeHaiku35,

    // OpenAI models (direct)
    Gpt52,
    Gpt51,
    Gpt5,
    Gpt5Mini,
    Gpt45,
    Gpt4o,
    Gpt4oMini,

    // OpenRouter models (proxied)
    OrClaudeOpus45,
    OrClaudeSonnet45,
    OrClaudeHaiku45,
    OrGpt52,
    OrGpt51,
    OrGpt5,
    OrGpt5Mini,
    OrGpt45,

    // Legacy/utility
    Fast,
    OpenaiFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Selection {
    pub rig: AomiModel,
    pub baml: AomiModel,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            rig: AomiModel::ClaudeOpus45,
            baml: AomiModel::ClaudeOpus45,
        }
    }
}

impl AomiModel {
    /// Check if model is currently supported
    pub const fn is_supported(self) -> bool {
        matches!(
            self,
            // Anthropic direct
            AomiModel::ClaudeOpus45
                | AomiModel::ClaudeSonnet45
                | AomiModel::ClaudeHaiku45
                | AomiModel::ClaudeOpus4
                | AomiModel::ClaudeSonnet4
                | AomiModel::ClaudeHaiku35
                // OpenAI direct
                | AomiModel::Gpt52
                | AomiModel::Gpt51
                | AomiModel::Gpt5
                | AomiModel::Gpt5Mini
                | AomiModel::Gpt45
                | AomiModel::Gpt4o
                | AomiModel::Gpt4oMini
                // OpenRouter
                | AomiModel::OrClaudeOpus45
                | AomiModel::OrClaudeSonnet45
                | AomiModel::OrClaudeHaiku45
                | AomiModel::OrGpt52
                | AomiModel::OrGpt51
                | AomiModel::OrGpt5
                | AomiModel::OrGpt5Mini
                | AomiModel::OrGpt45
        )
    }

    /// Returns the provider for this model
    pub const fn provider(self) -> Option<ModelProvider> {
        match self {
            // Anthropic direct
            AomiModel::ClaudeOpus45
            | AomiModel::ClaudeSonnet45
            | AomiModel::ClaudeHaiku45
            | AomiModel::ClaudeOpus4
            | AomiModel::ClaudeSonnet4
            | AomiModel::ClaudeHaiku35 => Some(ModelProvider::Anthropic),

            // OpenAI direct
            AomiModel::Gpt52
            | AomiModel::Gpt51
            | AomiModel::Gpt5
            | AomiModel::Gpt5Mini
            | AomiModel::Gpt45
            | AomiModel::Gpt4o
            | AomiModel::Gpt4oMini => Some(ModelProvider::OpenAI),

            // OpenRouter
            AomiModel::OrClaudeOpus45
            | AomiModel::OrClaudeSonnet45
            | AomiModel::OrClaudeHaiku45
            | AomiModel::OrGpt52
            | AomiModel::OrGpt51
            | AomiModel::OrGpt5
            | AomiModel::OrGpt5Mini
            | AomiModel::OrGpt45 => Some(ModelProvider::OpenRouter),

            AomiModel::Fast | AomiModel::OpenaiFallback => None,
        }
    }

    /// Legacy method for backward compatibility
    pub const fn rig_provider(self) -> Option<&'static str> {
        match self.provider() {
            Some(ModelProvider::Anthropic) => Some("anthropic"),
            Some(ModelProvider::OpenAI) => Some("openai"),
            Some(ModelProvider::OpenRouter) => Some("openrouter"),
            None => None,
        }
    }

    /// Returns the model ID for the provider's API
    pub const fn rig_id(self) -> &'static str {
        match self {
            // Anthropic models
            AomiModel::ClaudeOpus45 => "claude-opus-4-5-20251101",
            AomiModel::ClaudeSonnet45 => "claude-sonnet-4-5-20251101",
            AomiModel::ClaudeHaiku45 => "claude-haiku-4-5-20251101",
            AomiModel::ClaudeOpus4 => "claude-opus-4-1-20250805",
            AomiModel::ClaudeSonnet4 => "claude-sonnet-4-20250514",
            AomiModel::ClaudeHaiku35 => "claude-3-5-haiku-20241022",

            // OpenAI models
            AomiModel::Gpt52 => "gpt-5.2",
            AomiModel::Gpt51 => "gpt-5.1",
            AomiModel::Gpt5 => "gpt-5",
            AomiModel::Gpt5Mini => "gpt-5-mini",
            AomiModel::Gpt45 => "gpt-4.5-preview",
            AomiModel::Gpt4o => "gpt-4o",
            AomiModel::Gpt4oMini => "gpt-4o-mini",

            // OpenRouter models (use OpenRouter format: provider/model)
            AomiModel::OrClaudeOpus45 => "anthropic/claude-opus-4.5",
            AomiModel::OrClaudeSonnet45 => "anthropic/claude-sonnet-4.5",
            AomiModel::OrClaudeHaiku45 => "anthropic/claude-haiku-4.5",
            AomiModel::OrGpt52 => "openai/gpt-5.2",
            AomiModel::OrGpt51 => "openai/gpt-5.1",
            AomiModel::OrGpt5 => "openai/gpt-5",
            AomiModel::OrGpt5Mini => "openai/gpt-5-mini",
            AomiModel::OrGpt45 => "openai/gpt-4.5-preview",

            // Fallbacks
            AomiModel::Fast => "gpt-4o-mini",
            AomiModel::OpenaiFallback => "gpt-4o",
        }
    }

    pub const fn rig_slug(self) -> &'static str {
        match self {
            // Anthropic
            AomiModel::ClaudeOpus45 => "opus-4.5",
            AomiModel::ClaudeSonnet45 => "sonnet-4.5",
            AomiModel::ClaudeHaiku45 => "haiku-4.5",
            AomiModel::ClaudeOpus4 => "opus-4",
            AomiModel::ClaudeSonnet4 => "sonnet-4",
            AomiModel::ClaudeHaiku35 => "haiku-3.5",

            // OpenAI
            AomiModel::Gpt52 => "gpt-5.2",
            AomiModel::Gpt51 => "gpt-5.1",
            AomiModel::Gpt5 => "gpt-5",
            AomiModel::Gpt5Mini => "gpt-5-mini",
            AomiModel::Gpt45 => "gpt-4.5",
            AomiModel::Gpt4o => "gpt-4o",
            AomiModel::Gpt4oMini => "gpt-4o-mini",

            // OpenRouter
            AomiModel::OrClaudeOpus45 => "or-opus-4.5",
            AomiModel::OrClaudeSonnet45 => "or-sonnet-4.5",
            AomiModel::OrClaudeHaiku45 => "or-haiku-4.5",
            AomiModel::OrGpt52 => "or-gpt-5.2",
            AomiModel::OrGpt51 => "or-gpt-5.1",
            AomiModel::OrGpt5 => "or-gpt-5",
            AomiModel::OrGpt5Mini => "or-gpt-5-mini",
            AomiModel::OrGpt45 => "or-gpt-4.5",

            AomiModel::Fast => "fast",
            AomiModel::OpenaiFallback => "openai-fallback",
        }
    }

    pub const fn rig_label(self) -> &'static str {
        match self {
            // Anthropic
            AomiModel::ClaudeOpus45 => "Claude Opus 4.5 (MoE)",
            AomiModel::ClaudeSonnet45 => "Claude Sonnet 4.5",
            AomiModel::ClaudeHaiku45 => "Claude Haiku 4.5",
            AomiModel::ClaudeOpus4 => "Claude Opus 4.1",
            AomiModel::ClaudeSonnet4 => "Claude Sonnet 4",
            AomiModel::ClaudeHaiku35 => "Claude 3.5 Haiku",

            // OpenAI
            AomiModel::Gpt52 => "GPT-5.2",
            AomiModel::Gpt51 => "GPT-5.1",
            AomiModel::Gpt5 => "GPT-5",
            AomiModel::Gpt5Mini => "GPT-5 Mini",
            AomiModel::Gpt45 => "GPT-4.5 Preview",
            AomiModel::Gpt4o => "GPT-4o",
            AomiModel::Gpt4oMini => "GPT-4o Mini",

            // OpenRouter
            AomiModel::OrClaudeOpus45 => "Claude Opus 4.5 (OpenRouter)",
            AomiModel::OrClaudeSonnet45 => "Claude Sonnet 4.5 (OpenRouter)",
            AomiModel::OrClaudeHaiku45 => "Claude Haiku 4.5 (OpenRouter)",
            AomiModel::OrGpt52 => "GPT-5.2 (OpenRouter)",
            AomiModel::OrGpt51 => "GPT-5.1 (OpenRouter)",
            AomiModel::OrGpt5 => "GPT-5 (OpenRouter)",
            AomiModel::OrGpt5Mini => "GPT-5 Mini (OpenRouter)",
            AomiModel::OrGpt45 => "GPT-4.5 (OpenRouter)",

            AomiModel::Fast => "Fast Round Robin",
            AomiModel::OpenaiFallback => "OpenAI Fallback",
        }
    }

    pub const fn baml_client_name(self) -> &'static str {
        match self {
            AomiModel::ClaudeOpus45 => "CustomOpus45",
            AomiModel::ClaudeSonnet45 => "CustomSonnet45",
            AomiModel::ClaudeHaiku45 => "CustomHaiku45",
            AomiModel::ClaudeOpus4 => "CustomOpus4",
            AomiModel::ClaudeSonnet4 => "CustomSonnet4",
            AomiModel::ClaudeHaiku35 => "CustomHaiku",
            AomiModel::Gpt52 => "CustomGPT52",
            AomiModel::Gpt51 => "CustomGPT51",
            AomiModel::Gpt5 => "CustomGPT5",
            AomiModel::Gpt5Mini => "CustomGPT5Mini",
            AomiModel::Gpt45 => "CustomGPT45",
            AomiModel::Gpt4o => "CustomGPT4o",
            AomiModel::Gpt4oMini => "CustomGPT4oMini",
            AomiModel::OrClaudeOpus45 => "OrOpus45",
            AomiModel::OrClaudeSonnet45 => "OrSonnet45",
            AomiModel::OrClaudeHaiku45 => "OrHaiku45",
            AomiModel::OrGpt52 => "OrGPT52",
            AomiModel::OrGpt51 => "OrGPT51",
            AomiModel::OrGpt5 => "OrGPT5",
            AomiModel::OrGpt5Mini => "OrGPT5Mini",
            AomiModel::OrGpt45 => "OrGPT45",
            AomiModel::Fast => "CustomFast",
            AomiModel::OpenaiFallback => "OpenaiFallback",
        }
    }

    pub const fn baml_label(self) -> &'static str {
        self.rig_label()
    }

    pub fn parse_rig(input: &str) -> Option<Self> {
        let normalized = input.trim().to_lowercase();
        match normalized.as_str() {
            // Claude 4.5 (MoE)
            "opus-4.5" | "opus45" | "claude-opus-4.5" | "claude-opus-4-5-20251101" => {
                Some(AomiModel::ClaudeOpus45)
            }
            "sonnet-4.5" | "sonnet45" | "claude-sonnet-4.5" | "claude-sonnet-4-5-20251101" => {
                Some(AomiModel::ClaudeSonnet45)
            }
            "haiku-4.5" | "haiku45" | "claude-haiku-4.5" | "claude-haiku-4-5-20251101" => {
                Some(AomiModel::ClaudeHaiku45)
            }

            // Claude 4.x legacy
            "opus" | "opus-4" | "opus-4.1" | "claude-opus-4-1-20250805" => {
                Some(AomiModel::ClaudeOpus4)
            }
            "sonnet" | "sonnet-4" | "claude-sonnet-4-20250514" => Some(AomiModel::ClaudeSonnet4),
            "haiku" | "haiku-3-5" | "haiku-3.5" | "claude-3-5-haiku-20241022" => {
                Some(AomiModel::ClaudeHaiku35)
            }

            // GPT-5.x series
            "gpt-5.2" | "gpt52" | "gpt5.2" => Some(AomiModel::Gpt52),
            "gpt-5.1" | "gpt51" | "gpt5.1" => Some(AomiModel::Gpt51),
            "gpt-5" | "gpt5" => Some(AomiModel::Gpt5),
            "gpt-5-mini" | "gpt5-mini" | "gpt5mini" => Some(AomiModel::Gpt5Mini),
            "gpt-4.5" | "gpt45" | "gpt4.5" | "gpt-4.5-preview" => Some(AomiModel::Gpt45),
            "gpt-4o" | "gpt4o" => Some(AomiModel::Gpt4o),
            "gpt-4o-mini" | "gpt4o-mini" | "gpt4omini" => Some(AomiModel::Gpt4oMini),

            // OpenRouter models (prefixed with or-)
            "or-opus-4.5" | "or-opus45" | "openrouter-opus-4.5" => Some(AomiModel::OrClaudeOpus45),
            "or-sonnet-4.5" | "or-sonnet45" | "openrouter-sonnet-4.5" => {
                Some(AomiModel::OrClaudeSonnet45)
            }
            "or-haiku-4.5" | "or-haiku45" | "openrouter-haiku-4.5" => {
                Some(AomiModel::OrClaudeHaiku45)
            }
            "or-gpt-5.2" | "or-gpt52" | "openrouter-gpt-5.2" => Some(AomiModel::OrGpt52),
            "or-gpt-5.1" | "or-gpt51" | "openrouter-gpt-5.1" => Some(AomiModel::OrGpt51),
            "or-gpt-5" | "or-gpt5" | "openrouter-gpt-5" => Some(AomiModel::OrGpt5),
            "or-gpt-5-mini" | "or-gpt5-mini" | "openrouter-gpt-5-mini" => {
                Some(AomiModel::OrGpt5Mini)
            }
            "or-gpt-4.5" | "or-gpt45" | "openrouter-gpt-4.5" => Some(AomiModel::OrGpt45),

            // Utility
            "fast" | "fast round robin" => Some(AomiModel::Fast),
            "openai-fallback" | "openai fallback" | "fallback" => Some(AomiModel::OpenaiFallback),

            _ => None,
        }
    }

    pub fn parse_baml(input: &str) -> Option<Self> {
        // BAML parsing uses same logic as rig parsing
        Self::parse_rig(input)
    }

    pub const fn rig_all() -> &'static [AomiModel] {
        &[
            // Anthropic direct
            AomiModel::ClaudeOpus45,
            AomiModel::ClaudeSonnet45,
            AomiModel::ClaudeHaiku45,
            AomiModel::ClaudeOpus4,
            AomiModel::ClaudeSonnet4,
            AomiModel::ClaudeHaiku35,
            // OpenAI direct
            AomiModel::Gpt52,
            AomiModel::Gpt51,
            AomiModel::Gpt5,
            AomiModel::Gpt5Mini,
            AomiModel::Gpt45,
            AomiModel::Gpt4o,
            AomiModel::Gpt4oMini,
            // OpenRouter
            AomiModel::OrClaudeOpus45,
            AomiModel::OrClaudeSonnet45,
            AomiModel::OrClaudeHaiku45,
            AomiModel::OrGpt52,
            AomiModel::OrGpt51,
            AomiModel::OrGpt5,
            AomiModel::OrGpt5Mini,
            AomiModel::OrGpt45,
        ]
    }

    pub const fn baml_all() -> &'static [AomiModel] {
        &[
            AomiModel::ClaudeOpus45,
            AomiModel::ClaudeSonnet45,
            AomiModel::ClaudeHaiku45,
            AomiModel::ClaudeOpus4,
            AomiModel::ClaudeSonnet4,
            AomiModel::ClaudeHaiku35,
            AomiModel::Gpt52,
            AomiModel::Gpt51,
            AomiModel::Gpt5,
            AomiModel::Gpt5Mini,
            AomiModel::Gpt45,
            AomiModel::Gpt4o,
            AomiModel::Gpt4oMini,
            AomiModel::OrClaudeOpus45,
            AomiModel::OrClaudeSonnet45,
            AomiModel::OrClaudeHaiku45,
            AomiModel::OrGpt52,
            AomiModel::OrGpt51,
            AomiModel::OrGpt5,
            AomiModel::OrGpt5Mini,
            AomiModel::OrGpt45,
            AomiModel::Fast,
            AomiModel::OpenaiFallback,
        ]
    }

    /// Returns only Anthropic direct models
    pub const fn anthropic_models() -> &'static [AomiModel] {
        &[
            AomiModel::ClaudeOpus45,
            AomiModel::ClaudeSonnet45,
            AomiModel::ClaudeHaiku45,
            AomiModel::ClaudeOpus4,
            AomiModel::ClaudeSonnet4,
            AomiModel::ClaudeHaiku35,
        ]
    }

    /// Returns only OpenAI direct models
    pub const fn openai_models() -> &'static [AomiModel] {
        &[
            AomiModel::Gpt52,
            AomiModel::Gpt51,
            AomiModel::Gpt5,
            AomiModel::Gpt5Mini,
            AomiModel::Gpt45,
            AomiModel::Gpt4o,
            AomiModel::Gpt4oMini,
        ]
    }

    /// Returns only OpenRouter models
    pub const fn openrouter_models() -> &'static [AomiModel] {
        &[
            AomiModel::OrClaudeOpus45,
            AomiModel::OrClaudeSonnet45,
            AomiModel::OrClaudeHaiku45,
            AomiModel::OrGpt52,
            AomiModel::OrGpt51,
            AomiModel::OrGpt5,
            AomiModel::OrGpt5Mini,
            AomiModel::OrGpt45,
        ]
    }
}
