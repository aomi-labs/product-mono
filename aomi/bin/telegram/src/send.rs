use regex::Regex;

use teloxide::payloads::SendMessageSetters;
use teloxide::types::ThreadId;

pub fn with_thread_id<T>(request: T, thread_id: Option<ThreadId>) -> T
where
    T: SendMessageSetters,
{
    if let Some(thread_id) = thread_id {
        request.message_thread_id(thread_id)
    } else {
        request
    }
}

pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn markdown_to_telegram_html(markdown: &str) -> String {
    let escaped = escape_html(markdown);

    let code_re = Regex::new(r"`([^`]+)`").expect("valid regex");
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").expect("valid regex");
    let bold_re = Regex::new(r"\*\*([^*]+)\*\*").expect("valid regex");
    let italic_re = Regex::new(r"\*([^*]+)\*").expect("valid regex");

    let converted = code_re.replace_all(&escaped, "<code>$1</code>");
    let converted = link_re.replace_all(&converted, "<a href=\"$2\">$1</a>");
    let converted = bold_re.replace_all(&converted, "<b>$1</b>");
    let converted = italic_re.replace_all(&converted, "<i>$1</i>");

    converted.into_owned()
}

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
        // Check if adding this line (plus newline) would exceed limit
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
            // First, save current chunk if any
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

pub fn format_for_telegram(markdown: &str) -> Vec<String> {
    if markdown.trim().is_empty() {
        return vec![];
    }
    let html = markdown_to_telegram_html(markdown);
    chunk_message(&html, 4000)
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_replaces_entities() {
        let input = "Hello & <world> \"ok\"";
        let expected = "Hello &amp; &lt;world&gt; &quot;ok&quot;";
        assert_eq!(escape_html(input), expected);
    }

    #[test]
    fn markdown_to_telegram_html_converts_markup() {
        let input = "Hi & **bold** [link](https://a.com) *ital* `code`";
        let expected =
            "Hi &amp; <b>bold</b> <a href=\"https://a.com\">link</a> <i>ital</i> <code>code</code>";
        assert_eq!(markdown_to_telegram_html(input), expected);
    }

    #[test]
    fn chunk_message_preserves_newlines_when_fits() {
        let input = "first\nsecond\nthird";
        let chunks = chunk_message(input, 100);
        assert_eq!(chunks, vec!["first\nsecond\nthird".to_string()]);
    }

    #[test]
    fn chunk_message_splits_at_newlines_when_needed() {
        let input = "first\nsecond\nthird";
        let chunks = chunk_message(input, 12); // "first\nsecond" = 12 chars
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
    fn format_for_telegram_wraps_and_chunks() {
        let input = "*hi*";
        let chunks = format_for_telegram(input);
        assert_eq!(chunks, vec!["<i>hi</i>".to_string()]);
    }
}
