//! Context window management for LLM calls.
//!
//! Provides O(1) sliding window over conversation history to prevent context explosion
//! while preserving full history for persistence.

use rig::message::Message;

/// Default context budget (tokens). Claude 3.5 Sonnet supports 200k input.
/// We leave headroom for system prompt and output tokens.
pub const DEFAULT_CONTEXT_BUDGET: usize = 180_000;

/// Approximate tokens per character ratio for English text.
/// Conservative estimate: ~4 chars per token on average.
const CHARS_PER_TOKEN: usize = 4;

/// Estimates token count for a string.
/// Uses a simple heuristic: ~4 characters per token.
/// This is faster than calling a tokenizer and accurate enough for budgeting.
#[inline]
pub fn estimate_tokens(text: &str) -> usize {
    // Add 1 to avoid zero for very short strings
    (text.len() / CHARS_PER_TOKEN).max(1)
}

/// Estimates token count for a Message.
pub fn estimate_message_tokens(message: &Message) -> usize {
    match message {
        Message::User { content, .. } => {
            content.iter().map(estimate_user_content_tokens).sum()
        }
        Message::Assistant { content, .. } => {
            content.iter().map(estimate_assistant_content_tokens).sum()
        }
    }
}

/// Estimates tokens for user content variants.
fn estimate_user_content_tokens(content: &rig::message::UserContent) -> usize {
    use rig::message::UserContent;
    match content {
        UserContent::Text(text) => estimate_tokens(&text.text),
        UserContent::Image(_) => 85, // Default image token estimate
        UserContent::Audio(_) => 100, // Audio transcription estimate
        UserContent::Document(_) => 500, // Document content estimate
        UserContent::ToolResult(result) => {
            // Tool results: id + content
            let id_tokens = estimate_tokens(&result.id);
            let content_tokens: usize = result.content.iter()
                .map(|c| {
                    use rig::message::ToolResultContent;
                    match c {
                        ToolResultContent::Text(t) => estimate_tokens(&t.text),
                        ToolResultContent::Image(_) => 85,
                    }
                })
                .sum();
            id_tokens + content_tokens
        }
    }
}

/// Estimates tokens for assistant content variants.
fn estimate_assistant_content_tokens(content: &rig::message::AssistantContent) -> usize {
    use rig::message::AssistantContent;
    match content {
        AssistantContent::Text(text) => estimate_tokens(&text.text),
        AssistantContent::Reasoning(r) => estimate_tokens(&r.reasoning),
        AssistantContent::ToolCall(call) => {
            // Tool calls: name + arguments (serialized)
            let name_tokens = estimate_tokens(&call.function.name);
            let args_str = serde_json::to_string(&call.function.arguments).unwrap_or_default();
            let args_tokens = estimate_tokens(&args_str);
            name_tokens + args_tokens + 10 // 10 tokens overhead for structure
        }
    }
}

/// Cached token count for a message.
#[derive(Debug, Clone)]
pub struct TokenizedMessage {
    pub message: Message,
    pub token_count: usize,
}

impl TokenizedMessage {
    pub fn new(message: Message) -> Self {
        let token_count = estimate_message_tokens(&message);
        Self {
            message,
            token_count,
        }
    }
}

/// Sliding window over conversation history.
///
/// Maintains O(1) access to the context window by tracking:
/// - Full message history (for persistence)
/// - Window start index (first message in context)
/// - Running token count for the window
///
/// # Example
/// ```ignore
/// let mut window = ContextWindow::new(180_000);
/// window.push(Message::user("Hello"));
/// window.push(Message::assistant("Hi there!"));
///
/// // Get messages that fit in context budget
/// let context = window.get_context();
/// ```
#[derive(Debug, Clone)]
pub struct ContextWindow {
    /// All messages (full history for persistence)
    messages: Vec<TokenizedMessage>,
    /// Index of first message in the current context window
    window_start: usize,
    /// Total tokens in the current window
    window_tokens: usize,
    /// Maximum tokens allowed in the window
    context_budget: usize,
    /// Reserved tokens for system prompt (not counted against messages)
    system_prompt_reserve: usize,
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self::new(DEFAULT_CONTEXT_BUDGET)
    }
}

impl ContextWindow {
    /// Creates a new context window with the given token budget.
    pub fn new(context_budget: usize) -> Self {
        Self {
            messages: Vec::new(),
            window_start: 0,
            window_tokens: 0,
            context_budget,
            system_prompt_reserve: 10_000, // Reserve 10k for system prompt
        }
    }

    /// Creates a context window with custom system prompt reserve.
    pub fn with_system_reserve(context_budget: usize, system_prompt_reserve: usize) -> Self {
        Self {
            messages: Vec::new(),
            window_start: 0,
            window_tokens: 0,
            context_budget,
            system_prompt_reserve,
        }
    }

    /// Returns the effective budget for messages (total - system reserve).
    fn effective_budget(&self) -> usize {
        self.context_budget.saturating_sub(self.system_prompt_reserve)
    }

    /// Adds a message to the history and updates the sliding window.
    /// O(1) amortized - only slides window forward when over budget.
    pub fn push(&mut self, message: Message) {
        let tokenized = TokenizedMessage::new(message);
        self.window_tokens += tokenized.token_count;
        self.messages.push(tokenized);

        // Slide window forward if over budget
        self.slide_window();
    }

    /// Slides the window forward until within budget.
    fn slide_window(&mut self) {
        let budget = self.effective_budget();
        while self.window_tokens > budget && self.window_start < self.messages.len() {
            self.window_tokens -= self.messages[self.window_start].token_count;
            self.window_start += 1;
        }
    }

    /// Returns messages within the current context window.
    /// O(1) - returns a slice reference.
    pub fn get_context(&self) -> Vec<Message> {
        self.messages[self.window_start..]
            .iter()
            .map(|tm| tm.message.clone())
            .collect()
    }

    /// Returns all messages (full history for persistence).
    pub fn get_all_messages(&self) -> Vec<Message> {
        self.messages.iter().map(|tm| tm.message.clone()).collect()
    }

    /// Returns the number of messages in the context window.
    pub fn context_len(&self) -> usize {
        self.messages.len() - self.window_start
    }

    /// Returns the total number of messages (including those outside window).
    pub fn total_len(&self) -> usize {
        self.messages.len()
    }

    /// Returns the current token count in the window.
    pub fn window_tokens(&self) -> usize {
        self.window_tokens
    }

    /// Returns the context budget.
    pub fn context_budget(&self) -> usize {
        self.context_budget
    }

    /// Returns true if the window is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clears all messages and resets the window.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.window_start = 0;
        self.window_tokens = 0;
    }

    /// Initializes the window from existing messages.
    /// Computes token counts and sets up the sliding window.
    pub fn from_messages(messages: Vec<Message>, context_budget: usize) -> Self {
        let mut window = Self::new(context_budget);
        for message in messages {
            window.push(message);
        }
        window
    }

    /// Updates the last message if it's still streaming.
    /// Returns true if update was successful.
    pub fn update_last(&mut self, new_content: Message) -> bool {
        if let Some(last) = self.messages.last_mut() {
            let old_tokens = last.token_count;
            let new_tokenized = TokenizedMessage::new(new_content);
            self.window_tokens = self.window_tokens.saturating_sub(old_tokens) + new_tokenized.token_count;
            *last = new_tokenized;
            self.slide_window();
            true
        } else {
            false
        }
    }

    /// Removes the last message and returns it.
    pub fn pop(&mut self) -> Option<Message> {
        if let Some(tokenized) = self.messages.pop() {
            // Adjust window if we popped a message that was in the window
            if self.messages.len() >= self.window_start {
                self.window_tokens = self.window_tokens.saturating_sub(tokenized.token_count);
            }
            // Adjust window_start if needed
            if self.window_start > self.messages.len() {
                self.window_start = self.messages.len();
            }
            Some(tokenized.message)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        // 4 chars per token
        assert_eq!(estimate_tokens("hello"), 1);
        assert_eq!(estimate_tokens("hello world"), 2);
        assert_eq!(estimate_tokens("this is a longer message"), 6);
    }

    #[test]
    fn test_context_window_basic() {
        let mut window = ContextWindow::new(1000);
        assert!(window.is_empty());

        window.push(Message::user("Hello"));
        assert_eq!(window.total_len(), 1);
        assert_eq!(window.context_len(), 1);
    }

    #[test]
    fn test_context_window_sliding() {
        // Small budget to force sliding
        let mut window = ContextWindow::with_system_reserve(100, 0);

        // Add messages that will exceed budget
        for i in 0..10 {
            window.push(Message::user(format!(
                "This is message number {} with some extra content to use up tokens",
                i
            )));
        }

        // Window should have slid forward
        assert!(window.window_start > 0);
        assert!(window.window_tokens() <= 100);
        // But full history is preserved
        assert_eq!(window.total_len(), 10);
    }

    #[test]
    fn test_context_window_from_messages() {
        let messages = vec![
            Message::user("First message"),
            Message::assistant("Response"),
            Message::user("Follow up"),
        ];

        let window = ContextWindow::from_messages(messages, 10000);
        assert_eq!(window.total_len(), 3);
        assert_eq!(window.context_len(), 3);
    }

    #[test]
    fn test_context_window_pop() {
        let mut window = ContextWindow::new(10000);
        window.push(Message::user("Hello"));
        window.push(Message::assistant("Hi"));

        let popped = window.pop();
        assert!(popped.is_some());
        assert_eq!(window.total_len(), 1);
    }
}
