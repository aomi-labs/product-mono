//! Message formatting and chunking for Discord.
//!
//! Discord supports Markdown natively, so we don't need format conversion.
//! We just need to handle message chunking for the 2000 character limit.

/// Maximum message length for Discord
pub const MAX_MESSAGE_LENGTH: usize = 2000;

/// Chunk a message into Discord-compatible sizes.
///
/// Preserves newlines within chunks, only splitting when approaching
/// the 2000 character limit. Tries to split at newline boundaries
/// when possible for cleaner output.
pub fn chunk_message(text: &str, max_length: usize) -> Vec<String> {
    if max_length == 0 {
        return vec![String::new()];
    }

    if text.chars().count() <= max_length {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for line in text.split('\n') {
        let line_len = line.chars().count();
        let current_len = current_chunk.chars().count();
        let would_be = if current_chunk.is_empty() {
            line_len
        } else {
            current_len + 1 + line_len // +1 for newline
        };

        if would_be <= max_length {
            // Fits in current chunk
            if !current_chunk.is_empty() {
                current_chunk.push('\n');
            }
            current_chunk.push_str(line);
        } else if line_len > max_length {
            // Line itself is too long, need to split it
            if !current_chunk.is_empty() {
                chunks.push(std::mem::take(&mut current_chunk));
            }
            // Split the long line
            let line_chars: Vec<char> = line.chars().collect();
            let mut start = 0;
            while start < line_chars.len() {
                let end = (start + max_length).min(line_chars.len());
                let chunk: String = line_chars[start..end].iter().collect();
                if start + max_length >= line_chars.len() {
                    // Last piece of this line, start new chunk with it
                    current_chunk = chunk;
                } else {
                    chunks.push(chunk);
                }
                start = end;
            }
        } else {
            // Line doesn't fit, start new chunk
            if !current_chunk.is_empty() {
                chunks.push(std::mem::take(&mut current_chunk));
            }
            current_chunk = line.to_string();
        }
    }

    // Don't forget the last chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Format a message for Discord.
///
/// Discord supports Markdown natively, so we just need to chunk the message.
/// Returns empty vec for empty/whitespace-only input.
pub fn format_for_discord(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![];
    }
    chunk_message(text, MAX_MESSAGE_LENGTH)
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_message_preserves_newlines_when_fits() {
        let input = "first\nsecond\nthird";
        let chunks = chunk_message(input, 100);
        assert_eq!(chunks, vec!["first\nsecond\nthird".to_string()]);
    }

    #[test]
    fn chunk_message_splits_at_newlines_when_needed() {
        let input = "first\nsecond\nthird";
        let chunks = chunk_message(input, 12);
        assert_eq!(
            chunks,
            vec!["first\nsecond".to_string(), "third".to_string()]
        );
    }

    #[test]
    fn chunk_message_splits_long_lines() {
        let input = "abcdef";
        let chunks = chunk_message(input, 2);
        assert_eq!(
            chunks,
            vec!["ab".to_string(), "cd".to_string(), "ef".to_string()]
        );
    }

    #[test]
    fn format_for_discord_filters_empty() {
        let chunks = format_for_discord("   \n\n   ");
        assert!(chunks.is_empty());
    }

    #[test]
    fn format_for_discord_preserves_markdown() {
        let input = "**bold** and *italic* and `code`";
        let chunks = format_for_discord(input);
        assert_eq!(chunks, vec!["**bold** and *italic* and `code`".to_string()]);
    }
}
