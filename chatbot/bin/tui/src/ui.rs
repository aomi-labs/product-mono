use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, MessageSender};

const MAX_WIDTH: u16 = 120;

pub fn draw(f: &mut Frame, app: &mut App) {
    let mut full_area = f.area();
    full_area.height = full_area.height.saturating_sub(2);

    let area = center_area(full_area, MAX_WIDTH);
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title_bottom(" <↑ ↓> scroll • <esc> interrupt • <ctrl-c> quit ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(Color::White));
    f.render_widget(outer_block.clone(), area);

    let inner_area = outer_block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(inner_area);

    draw_messages(f, app, chunks[0]);
    draw_input(f, app, chunks[1]);
}

fn center_area(area: Rect, max_width: u16) -> Rect {
    let width = area.width.min(max_width);
    let x = (area.width.saturating_sub(width)) / 2;

    Rect {
        x: area.x + x,
        y: area.y,
        width,
        height: area.height,
    }
}

fn draw_messages(f: &mut Frame, app: &mut App, area: Rect) {
    let padded_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let available_width = padded_area.width as usize;
    let max_message_width = (available_width * 2 / 3).min(60);

    let mut list_items = Vec::new();

    for (i, msg) in app.messages.iter().enumerate() {
        if msg.content.is_empty() && !msg.is_streaming {
            continue;
        }

        let wrapped_lines = if msg.content.is_empty() {
            vec!["".into()]
        } else {
            wrap(&msg.content, max_message_width)
        };

        match msg.sender {
            MessageSender::User => {
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
            MessageSender::Assistant => {
                let actual_width = wrapped_lines
                    .iter()
                    .map(|line| line.width())
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

                if wrapped_lines.is_empty()
                    || (wrapped_lines.len() == 1 && wrapped_lines[0].is_empty() && msg.is_streaming)
                {
                    list_items.push(ListItem::new(Line::from(vec![Span::styled(
                        "│  │",
                        Style::default().fg(Color::White),
                    )])));
                } else {
                    for line in wrapped_lines.iter() {
                        let line_text = line.to_string();
                        let line_width = line_text.width();
                        let line_padding = actual_width.saturating_sub(line_width);
                        let padded_content = format!("{}{}", line_text, " ".repeat(line_padding));

                        list_items.push(ListItem::new(Line::from(vec![Span::styled(
                            format!("│ {padded_content} │"),
                            Style::default().fg(Color::White),
                        )])));
                    }
                }

                let bottom_border = format!("╰{}╯", "─".repeat(bubble_width - 2));
                list_items.push(ListItem::new(Line::from(vec![Span::styled(
                    bottom_border,
                    Style::default().fg(Color::White),
                )])));

                let is_last_assistant = app
                    .messages
                    .iter()
                    .rposition(|m| matches!(m.sender, MessageSender::Assistant))
                    .map(|idx| std::ptr::eq(&app.messages[idx], msg))
                    .unwrap_or(false);

                if is_last_assistant {
                    let spinner_chars = ["≽^•ᴗ•^≼", "≽^•о•^≼", "≽^•⩊•^≼"];
                    let spinner = spinner_chars[app.spinner_index % spinner_chars.len()];
                    list_items.push(ListItem::new(Line::from(vec![if msg.is_streaming {
                        Span::styled(
                            spinner,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        )
                    } else {
                        Span::styled("≽^•⩊•^≼", Style::default().fg(Color::Cyan))
                    }])));
                }
            }
            MessageSender::System => {
                for line in wrapped_lines {
                    let padding_left =
                        " ".repeat((available_width.saturating_sub(line.width())) / 2);
                    let styled_line = Line::from(vec![
                        Span::raw(padding_left),
                        Span::styled(
                            line.to_string(),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(ratatui::style::Modifier::DIM),
                        ),
                    ]);
                    list_items.push(ListItem::new(styled_line));
                }
            }
        }

        if i + 1 < app.messages.len() {
            list_items.push(ListItem::new(Line::from("")));
        }
    }

    app.total_list_items = list_items.len();
    let visible_items = padded_area.height as usize;

    if app.auto_scroll && !app.messages.is_empty() {
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

    let start = app.scroll_offset;
    let end = (start + visible_items).min(list_items.len());
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

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let padded_area = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().fg(if app.is_processing {
            Color::Cyan
        } else {
            Color::White
        }));

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(input_block.clone())
        .wrap(Wrap { trim: false });

    f.render_widget(input, padded_area);

    if !app.is_processing {
        let inner_area = input_block.inner(padded_area);
        let cursor_x = inner_area.x + app.cursor_position as u16;
        let cursor_y = inner_area.y;
        f.set_cursor_position((cursor_x.min(inner_area.x + inner_area.width - 1), cursor_y));
    }
}
