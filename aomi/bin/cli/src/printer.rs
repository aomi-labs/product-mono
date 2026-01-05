use std::io::{self, Write};

use aomi_backend::{ChatMessage, MessageSender};
use aomi_chat::SystemEvent;
use colored::Colorize;
use serde_json::Value;

#[derive(Default)]
struct MessageState {
    printed_header: bool,
    printed_len: usize,
    finished: bool,
}

pub struct MessagePrinter {
    states: Vec<MessageState>,
    show_tool_content: bool,
}

impl MessagePrinter {
    pub fn new(show_tool_content: bool) -> Self {
        Self {
            states: Vec::new(),
            show_tool_content,
        }
    }

    pub fn render(&mut self, messages: &[ChatMessage]) -> io::Result<()> {
        for (idx, msg) in messages.iter().enumerate() {
            if idx >= self.states.len() {
                self.states.push(MessageState::default());
            }
            self.render_message(idx, msg)?;
        }
        Ok(())
    }

    fn render_message(&mut self, idx: usize, message: &ChatMessage) -> io::Result<()> {
        let state = &mut self.states[idx];
        if state.finished && !message.is_streaming {
            return Ok(());
        }

        let (text, tool_topic) = match &message.tool_stream {
            Some((topic, content)) => (content.as_str(), Some(topic.as_str())),
            None => (message.content.as_str(), None),
        };
        let is_tool = tool_topic.is_some();

        if is_tool && !self.show_tool_content {
            if !state.printed_header {
                let mut stdout = io::stdout();
                writeln!(stdout, "{}", format_header(message, tool_topic))?;
                stdout.flush()?;
                state.printed_header = true;
            }
            state.finished = true;
            state.printed_len = text.len();
            return Ok(());
        }

        if !state.printed_header {
            let mut stdout = io::stdout();
            let header = format!("{} ", format_header(message, tool_topic));

            if message.is_streaming || is_tool {
                write!(stdout, "{header}")?;
                stdout.flush()?;
            } else {
                writeln!(stdout, "{header}{text}")?;
                stdout.flush()?;
                state.printed_len = text.len();
                state.finished = true;
                state.printed_header = true;
                return Ok(());
            }

            state.printed_header = true;
        }

        if text.len() > state.printed_len {
            let new_chunk = &text[state.printed_len..];
            let mut stdout = io::stdout();
            write!(stdout, "{new_chunk}")?;
            stdout.flush()?;
            state.printed_len = text.len();
        }

        if !message.is_streaming && !state.finished {
            let mut stdout = io::stdout();
            writeln!(stdout)?;
            stdout.flush()?;
            state.finished = true;
        }

        Ok(())
    }
}

fn format_header(message: &ChatMessage, tool_topic: Option<&str>) -> String {
    let ts = message.timestamp.clone();
    match (tool_topic, &message.sender) {
        (Some(topic), _) => format!(
            "{} {}",
            format!("[{ts}]").dimmed(),
            format!("[tool:{topic}]").bold().yellow()
        ),
        (_, MessageSender::User) => format!(
            "{} {}",
            format!("[{ts}]").dimmed(),
            "[user]".bold().bright_cyan()
        ),
        (_, MessageSender::Assistant) => format!(
            "{} {}",
            format!("[{ts}]").dimmed(),
            "[assistant]".bold().green()
        ),
        (_, MessageSender::System) => format!(
            "{} {}",
            format!("[{ts}]").dimmed(),
            "[system]".bold().magenta()
        ),
    }
}

/// Render system events (inline and async updates)
pub fn render_system_events(
    inline_events: &[SystemEvent],
    async_updates: &[Value],
) -> io::Result<()> {
    let mut stdout = io::stdout();

    for event in inline_events {
        match event {
            SystemEvent::InlineDisplay(value) => {
                let summary = summarize_json(value);
                writeln!(
                    stdout,
                    "{}",
                    format!("[system:inline {}]", summary).magenta()
                )?;
            }
            SystemEvent::SystemNotice(msg) => {
                writeln!(stdout, "{}", format!("[system:notice {}]", msg).cyan())?;
            }
            SystemEvent::SystemError(msg) => {
                writeln!(stdout, "{}", format!("[system:error {}]", msg).red())?;
            }
            SystemEvent::AsyncUpdate(_) => {
                // AsyncUpdate shouldn't appear in inline_events, skip
            }
        }
    }

    for value in async_updates {
        let summary = summarize_json(value);
        writeln!(stdout, "{}", format!("[system:update {}]", summary).blue())?;
    }

    stdout.flush()?;
    Ok(())
}

/// Summarize JSON value for display (show type and key fields)
fn summarize_json(value: &Value) -> String {
    if let Some(obj) = value.as_object() {
        // Try to extract type and meaningful identifiers
        let event_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let mut parts = vec![event_type.to_string()];

        // Add tool_name if present
        if let Some(tool) = obj.get("tool_name").and_then(|v| v.as_str()) {
            parts.push(format!("tool:{}", tool));
        }

        // Add status if present
        if let Some(status) = obj.get("status").and_then(|v| v.as_str()) {
            parts.push(format!("status:{}", status));
        }

        // Add tx_hash if present (truncated)
        if let Some(hash) = obj.get("tx_hash").and_then(|v| v.as_str()) {
            let truncated = if hash.len() > 10 {
                format!("{}...", &hash[..10])
            } else {
                hash.to_string()
            };
            parts.push(format!("tx:{}", truncated));
        }

        parts.join(" ")
    } else {
        value.to_string().chars().take(50).collect()
    }
}
