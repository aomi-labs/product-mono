use regex::Regex;

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

    let mut chunks = Vec::new();
    for line in text.split('\n') {
        let line_chars: Vec<char> = line.chars().collect();
        if line_chars.len() <= max_length {
            chunks.push(line.to_string());
            continue;
        }

        let mut start = 0;
        while start < line_chars.len() {
            let end = (start + max_length).min(line_chars.len());
            let chunk: String = line_chars[start..end].iter().collect();
            chunks.push(chunk);
            start = end;
        }
    }
    chunks
}

pub fn format_for_telegram(markdown: &str) -> Vec<String> {
    let html = markdown_to_telegram_html(markdown);
    chunk_message(&html, 4000)
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
    fn chunk_message_splits_on_newlines() {
        let input = "first\nsecond\nthird";
        let chunks = chunk_message(input, 10);
        assert_eq!(
            chunks,
            vec!["first".to_string(), "second".to_string(), "third".to_string()]
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
