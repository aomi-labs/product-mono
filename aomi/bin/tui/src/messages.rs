use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, Paragraph},
};
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

use crate::app::{MessageSender, SessionContainer};

pub(super) fn draw_messages(f: &mut Frame, app: &mut SessionContainer, area: Rect) {
    let padded_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let available_width = padded_area.width as usize;
    let max_message_width = (available_width * 2 / 3).min(60);

    let mut list_items = Vec::new();

    {
        let messages = app.session.messages.clone();
        let total_messages = messages.len();
        let last_assistant_idx = messages
            .iter()
            .rposition(|m| matches!(m.sender, MessageSender::Assistant));

        for (i, msg) in messages.iter().enumerate() {
            if msg.content.is_empty() && !msg.is_streaming && msg.tool_stream.is_none() {
                continue;
            }

            let wrapped_lines = if msg.content.is_empty() {
                vec!["".into()]
            } else {
                wrap(&msg.content, max_message_width)
            };

            match msg.sender {
                MessageSender::User => {
                    render_user_message(
                        &mut list_items,
                        &wrapped_lines,
                        msg,
                        available_width,
                        max_message_width,
                    );
                }
                MessageSender::Assistant => {
                    let is_last_assistant = last_assistant_idx.map(|idx| idx == i).unwrap_or(false);
                    render_assistant_message(
                        &mut list_items,
                        &wrapped_lines,
                        msg,
                        max_message_width,
                        is_last_assistant,
                        app.spinner_index,
                    );
                }
                MessageSender::System => {
                    render_system_message(&mut list_items, &wrapped_lines, available_width);
                }
            }

            if i + 1 < total_messages {
                list_items.push(ListItem::new(Line::from("")));
            }
        }
    }

    app.total_list_items = list_items.len();
    update_scroll_state(app, padded_area.height as usize);

    let start = app.scroll_offset;
    let end = (start + padded_area.height as usize).min(list_items.len());
    let visible_list_items: Vec<ListItem> = list_items[start..end].to_vec();

    let messages_list = List::new(visible_list_items);

    let clear_lines: Vec<Line> = (0..padded_area.height)
        .map(|_| Line::from(" ".repeat(padded_area.width as usize)))
        .collect();
    let clear_text = Text::from(clear_lines);
    let clear = Paragraph::new(clear_text);
    f.render_widget(clear, padded_area);

    f.render_widget(messages_list, padded_area);
}

fn render_user_message(
    list_items: &mut Vec<ListItem>,
    wrapped_lines: &[std::borrow::Cow<'_, str>],
    msg: &crate::app::ChatMessage,
    available_width: usize,
    max_message_width: usize,
) {
    let actual_width = wrapped_lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(max_message_width);
    let bubble_width = actual_width + 4;
    let bubble_start = available_width.saturating_sub(bubble_width);

    let timestamp_text = msg.timestamp.clone();
    let timestamp_padding = available_width.saturating_sub(timestamp_text.len());
    list_items.push(ListItem::new(Line::from(vec![
        Span::raw(" ".repeat(timestamp_padding)),
        Span::styled(timestamp_text, Style::default().fg(Color::DarkGray)),
    ])));

    let top_border = format!("╭{}╮", "─".repeat(bubble_width - 2));
    list_items.push(ListItem::new(Line::from(vec![
        Span::raw(" ".repeat(bubble_start)),
        Span::styled(top_border, Style::default().fg(Color::Cyan)),
    ])));

    for line in wrapped_lines.iter() {
        let line_text = line.to_string();
        let line_width = line_text.width();
        let line_padding = actual_width.saturating_sub(line_width);
        let content = format!("│ {}{} │", line_text, " ".repeat(line_padding));

        list_items.push(ListItem::new(Line::from(vec![
            Span::raw(" ".repeat(bubble_start)),
            Span::styled(
                content.chars().take(1).collect::<String>(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                content
                    .chars()
                    .skip(1)
                    .take(content.len() - 2)
                    .collect::<String>(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                content.chars().skip(content.len() - 1).collect::<String>(),
                Style::default().fg(Color::Cyan),
            ),
        ])));
    }

    let bottom_border = format!("╰{}╯", "─".repeat(bubble_width - 2));
    list_items.push(ListItem::new(Line::from(vec![
        Span::raw(" ".repeat(bubble_start)),
        Span::styled(bottom_border, Style::default().fg(Color::Cyan)),
    ])));
}

fn render_assistant_message(
    list_items: &mut Vec<ListItem>,
    wrapped_lines: &[std::borrow::Cow<'_, str>],
    msg: &crate::app::ChatMessage,
    max_message_width: usize,
    is_last_assistant: bool,
    spinner_index: usize,
) {
    let mut bubble_lines: Vec<(String, Style)> = Vec::new();

    let has_wrapped_content = wrapped_lines.iter().any(|line| !line.is_empty());
    if has_wrapped_content {
        for line in wrapped_lines.iter() {
            bubble_lines.push((line.to_string(), Style::default().fg(Color::White)));
        }
    }

    if let Some((topic, stream_content)) = &msg.tool_stream {
        let topic_lines = wrap(topic, max_message_width);
        for line in topic_lines {
            bubble_lines.push((
                line.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let stream_lines = wrap(stream_content, max_message_width);
        for line in stream_lines {
            bubble_lines.push((
                line.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));
        }
    }

    let has_content = !bubble_lines.is_empty();
    let actual_width = bubble_lines
        .iter()
        .map(|(line, _)| line.width())
        .max()
        .unwrap_or(0)
        .min(max_message_width);
    let bubble_width = actual_width + 4;

    list_items.push(ListItem::new(Line::from(vec![Span::styled(
        msg.timestamp.clone(),
        Style::default().fg(Color::DarkGray),
    )])));

    let top_border = format!("╭{}╮", "─".repeat(bubble_width - 2));
    list_items.push(ListItem::new(Line::from(vec![Span::styled(
        top_border,
        Style::default().fg(Color::White),
    )])));

    if !has_content && msg.is_streaming {
        list_items.push(ListItem::new(Line::from(vec![Span::styled(
            "│  │",
            Style::default().fg(Color::White),
        )])));
    } else {
        let border_style = Style::default().fg(Color::White);
        for (text, style) in bubble_lines {
            let line_width = text.width();
            let line_padding = actual_width.saturating_sub(line_width);
            let padded_content = format!("{}{}", text, " ".repeat(line_padding));

            list_items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(padded_content, style),
                Span::styled(" │", border_style),
            ])));
        }
    }

    let bottom_border = format!("╰{}╯", "─".repeat(bubble_width - 2));
    list_items.push(ListItem::new(Line::from(vec![Span::styled(
        bottom_border,
        Style::default().fg(Color::White),
    )])));

    if is_last_assistant {
        let spinner_chars = ["≽^•ᴗ•^≼", "≽^•о•^≼", "≽^•⩊•^≼"];
        let spinner = spinner_chars[spinner_index % spinner_chars.len()];
        list_items.push(ListItem::new(Line::from(vec![if msg.is_streaming {
            Span::styled(
                spinner,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("≽^•⩊•^≼", Style::default().fg(Color::Cyan))
        }])));
    }
}

fn render_system_message(
    list_items: &mut Vec<ListItem>,
    wrapped_lines: &[std::borrow::Cow<'_, str>],
    available_width: usize,
) {
    for line in wrapped_lines {
        let padding_left = " ".repeat((available_width.saturating_sub(line.width())) / 2);
        let styled_line = Line::from(vec![
            Span::raw(padding_left),
            Span::styled(
                line.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ]);
        list_items.push(ListItem::new(styled_line));
    }
}

fn update_scroll_state(app: &mut SessionContainer, visible_items: usize) {
    let has_messages = !app.session.messages.is_empty();

    if app.auto_scroll && has_messages {
        if app.total_list_items > visible_items {
            app.scroll_offset = app.total_list_items.saturating_sub(visible_items);
        } else {
            app.scroll_offset = 0;
        }
    }

    if app.total_list_items > visible_items {
        let max_offset = app.total_list_items.saturating_sub(visible_items);
        app.scroll_offset = app.scroll_offset.min(max_offset);
    } else {
        app.scroll_offset = 0;
    }
}
