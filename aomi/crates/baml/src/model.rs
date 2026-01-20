#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AomiModel {
    ClaudeSonnet4,
    ClaudeOpus4,
    ClaudeHaiku35,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub rig: AomiModel,
    pub baml: AomiModel,
}

impl AomiModel {
    pub const fn rig_id(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "claude-sonnet-4-20250514",
            AomiModel::ClaudeOpus4 => "claude-opus-4-1-20250805",
            AomiModel::ClaudeHaiku35 => "claude-3-5-haiku-20241022",
        }
    }

    pub const fn rig_slug(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "sonnet-4",
            AomiModel::ClaudeOpus4 => "opus-4",
            AomiModel::ClaudeHaiku35 => "haiku-3-5",
        }
    }

    pub const fn rig_label(self) -> &'static str {
        match self {
            AomiModel::ClaudeSonnet4 => "Claude Sonnet 4",
            AomiModel::ClaudeOpus4 => "Claude Opus 4.1",
            AomiModel::ClaudeHaiku35 => "Claude 3.5 Haiku",
        }
    }

    pub const fn baml_client_name(self) -> &'static str {
        "DefaultOpus4"
    }

    pub const fn baml_label(self) -> &'static str {
        "Claude Opus 4.1"
    }

    pub fn parse_rig(input: &str) -> Option<Self> {
        let normalized = input.trim().to_lowercase();
        match normalized.as_str() {
            "sonnet" | "sonnet-4" | "claude-sonnet-4-20250514" => Some(AomiModel::ClaudeSonnet4),
            "opus" | "opus-4" | "opus-4.1" | "claude-opus-4-1-20250805" => {
                Some(AomiModel::ClaudeOpus4)
            }
            "haiku" | "haiku-3-5" | "claude-3-5-haiku-20241022" => Some(AomiModel::ClaudeHaiku35),
            _ => None,
        }
    }

    pub fn parse_baml(input: &str) -> Option<Self> {
        let normalized = input.trim().to_lowercase();
        match normalized.as_str() {
            "opus" | "opus-4" | "defaultopus4" => Some(AomiModel::ClaudeOpus4),
            _ => None,
        }
    }

    pub const fn rig_all() -> &'static [AomiModel] {
        &[AomiModel::ClaudeSonnet4, AomiModel::ClaudeOpus4, AomiModel::ClaudeHaiku35]
    }

    pub const fn baml_all() -> &'static [AomiModel] {
        &[AomiModel::ClaudeOpus4]
    }
}
