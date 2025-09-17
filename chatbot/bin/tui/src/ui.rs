use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use textwrap::wrap;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, MessageSender};

const MAX_WIDTH: u16 = 120; // Roughly 960px at 8px per character
static MCP_SERVER_PORT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("MCP_SERVER_PORT").unwrap_or_else(|_| "5000".to_string())
});

pub fn draw(f: &mut Frame, app: &mut App) {
    // Get the terminal area but reserve bottom 4 lines for embedding download output
    let mut full_area = f.area();
    full_area.height = full_area.height.saturating_sub(4);

    // Center the TUI with max width
    let area = center_area(full_area, MAX_WIDTH);

    // Add outer border with instructions
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

    // Draw overlays with priority: API key > MCP connection > document loading
    if app.missing_api_key {
        draw_missing_api_key_overlay(f, app, full_area);
    } else if app.is_connecting_mcp {
        draw_mcp_connection_overlay(f, app, full_area);
    } else if app.is_loading {
        draw_loading_overlay(f, app, full_area);
    }
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
    // No border block, use the area directly

    // Add padding around the messages
    let padded_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    // Calculate available width for messages (account for padding)
    let available_width = padded_area.width as usize;
    let max_message_width = (available_width * 2 / 3).min(60); // Smaller max width for better chat bubbles

    let mut list_items = Vec::new();

    for (i, msg) in app.messages.iter().enumerate() {
        // Skip empty messages
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
                // Calculate actual message width for this specific message
                let actual_message_width = wrapped_lines
                    .iter()
                    .map(|line| line.width())
                    .max()
                    .unwrap_or(0)
                    .min(max_message_width);

                // Calculate bubble position - right aligned
                let bubble_width = actual_message_width + 4; // +4 for borders and padding
                let bubble_start = available_width.saturating_sub(bubble_width);

                // Timestamp above message, right-aligned
                let timestamp_text = msg.timestamp.clone();
                let timestamp_padding = available_width.saturating_sub(timestamp_text.len());
                list_items.push(ListItem::new(Line::from(vec![
                    Span::raw(" ".repeat(timestamp_padding)),
                    Span::styled(timestamp_text, Style::default().fg(Color::DarkGray)),
                ])));

                // Top border of bubble with rounded corners
                let top_border = format!("╭{}╮", "─".repeat(bubble_width - 2));
                list_items.push(ListItem::new(Line::from(vec![
                    Span::raw(" ".repeat(bubble_start)),
                    Span::styled(top_border, Style::default().fg(Color::Cyan)),
                ])));

                // Message lines with borders
                for line in wrapped_lines.iter() {
                    let line_text = line.to_string();
                    let line_width = line_text.width();
                    let line_padding = actual_message_width.saturating_sub(line_width);
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

                // Bottom border of bubble with rounded corners
                let bottom_border = format!("╰{}╯", "─".repeat(bubble_width - 2));
                list_items.push(ListItem::new(Line::from(vec![
                    Span::raw(" ".repeat(bubble_start)),
                    Span::styled(bottom_border, Style::default().fg(Color::Cyan)),
                ])));
            }
            MessageSender::Assistant => {
                // Calculate actual message width for agent messages too
                let actual_message_width = wrapped_lines
                    .iter()
                    .map(|line| line.width())
                    .max()
                    .unwrap_or(0)
                    .min(max_message_width);

                let bubble_width = actual_message_width + 4; // +4 for borders and padding

                // Timestamp above message, left-aligned
                list_items.push(ListItem::new(Line::from(vec![Span::styled(
                    msg.timestamp.clone(),
                    Style::default().fg(Color::DarkGray),
                )])));

                // Top border of bubble with rounded corners
                let top_border = format!("╭{}╮", "─".repeat(bubble_width - 2));
                list_items.push(ListItem::new(Line::from(vec![Span::styled(
                    top_border,
                    Style::default().fg(Color::White),
                )])));

                // Assistant messages on the left with borders
                // Handle empty streaming messages
                if wrapped_lines.is_empty()
                    || (wrapped_lines.len() == 1 && wrapped_lines[0].is_empty() && msg.is_streaming)
                {
                    // Just show an empty line for now
                    list_items.push(ListItem::new(Line::from(vec![Span::styled(
                        "│  │",
                        Style::default().fg(Color::White),
                    )])));
                } else {
                    // Track if we're in a tool call section
                    let mut in_tool_call = false;

                    for line in wrapped_lines.iter() {
                        let line_text = line.to_string();
                        let line_width = line_text.width();
                        let line_padding = actual_message_width.saturating_sub(line_width);
                        let padded_content = format!("{}{}", line_text, " ".repeat(line_padding));

                        // Check if this starts a new tool call
                        if line_text.trim().starts_with("→ Executing") {
                            in_tool_call = true;
                        } else if in_tool_call && line_text.trim().is_empty() {
                            // Empty line might end a tool call
                            in_tool_call = false;
                        }

                        if in_tool_call {
                            // For tool calls, style borders separately from content
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(Color::White)),
                                Span::styled(padded_content, Style::default().fg(Color::DarkGray)),
                                Span::styled(" │", Style::default().fg(Color::White)),
                            ])));
                        } else {
                            list_items.push(ListItem::new(Line::from(vec![Span::styled(
                                format!("│ {padded_content} │"),
                                Style::default().fg(Color::White),
                            )])));
                        }
                    }
                }

                // Bottom border of bubble with rounded corners
                let bottom_border = format!("╰{}╯", "─".repeat(bubble_width - 2));
                list_items.push(ListItem::new(Line::from(vec![Span::styled(
                    bottom_border,
                    Style::default().fg(Color::White),
                )])));

                // Only add cat if this is the most recent assistant message
                let is_last_assistant = app
                    .messages
                    .iter()
                    .rposition(|m| matches!(m.sender, MessageSender::Assistant))
                    .map(|idx| std::ptr::eq(&app.messages[idx], msg))
                    .unwrap_or(false);

                if is_last_assistant {
                    // Add cat below the bubble
                    list_items.push(ListItem::new(Line::from(vec![if msg.is_streaming {
                        // Show animated cat with different mouth expressions
                        let spinner_chars = [
                            "≽^•ᴗ•^≼", // small mouth
                            "≽^•о•^≼", // small o
                            "≽^•⩊•^≼", // closed mouth
                        ];
                        let spinner = spinner_chars[app.spinner_index % spinner_chars.len()];
                        Span::styled(
                            spinner,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        // Show resting cat when done
                        Span::styled("≽^•⩊•^≼", Style::default().fg(Color::Cyan))
                    }])));
                }
            }
            MessageSender::System => {
                // Check if this is a tool call or tool result
                let is_tool_call = msg.content.starts_with("tool:");
                // Tool results are now just the content, check if prev was a tool call
                let is_tool_result = if i > 0 {
                    if let Some(prev_msg) = app.messages.get(i - 1) {
                        prev_msg.sender == MessageSender::System
                            && prev_msg.content.starts_with("tool:")
                            && msg.sender == MessageSender::System
                            && !msg.content.starts_with("tool:")
                            && !msg.content.starts_with("error:")
                    } else {
                        false
                    }
                } else {
                    false
                };
                let is_tool_error = msg.content.starts_with("error:");
                let is_tool_message = is_tool_call || is_tool_result || is_tool_error;

                if is_tool_message {
                    // Tool messages: wrap at full width, with arrow indicators
                    let arrow = if is_tool_call {
                        "→ " // Straight arrow for tool calls
                    } else {
                        "↪ " // Curved arrow for results/errors
                    };

                    let text_color = if is_tool_error {
                        Color::Red
                    } else {
                        Color::DarkGray
                    };

                    // Add arrow to first line, then wrap remaining content
                    let content_with_arrow = format!("{}{}", arrow, msg.content);
                    let tool_wrapped_lines = wrap(&content_with_arrow, available_width);

                    // Render each wrapped line
                    for (line_idx, line) in tool_wrapped_lines.iter().enumerate() {
                        // For continuation lines of tool calls, add some indentation
                        let display_line = if line_idx > 0 && is_tool_call {
                            format!("  {line}") // Indent continuation lines
                        } else {
                            line.to_string()
                        };

                        list_items.push(ListItem::new(Line::from(vec![Span::styled(
                            display_line,
                            Style::default().fg(text_color),
                        )])));
                    }
                } else {
                    // Regular system messages - centered, no border
                    for line in wrapped_lines {
                        let padding_left =
                            " ".repeat((available_width.saturating_sub(line.width())) / 2);
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
            }
        }

        // Add spacing between messages, but not between tool call and its result
        let should_add_spacing = {
            // Check if current message is a tool call and next is a tool result
            if msg.sender == MessageSender::System && msg.content.starts_with("tool:") {
                // Check if next message exists and is a system message (likely a tool result)
                if let Some(next_msg) = app.messages.get(i + 1) {
                    // Don't add spacing if next is a system message that doesn't start with "call:"
                    next_msg.sender != MessageSender::System
                        || next_msg.content.starts_with("tool:")
                } else {
                    true
                }
            } else {
                true
            }
        };

        if should_add_spacing {
            list_items.push(ListItem::new(Line::from("")));
        }
    }

    // Update total items count for scrolling
    app.total_list_items = list_items.len();

    // Calculate visible items based on area height
    let visible_items = padded_area.height as usize;

    // Auto-scroll to bottom if auto_scroll is enabled
    if app.auto_scroll && !app.messages.is_empty() {
        // Always scroll to show the latest content
        if app.total_list_items > visible_items {
            app.scroll_offset = app.total_list_items.saturating_sub(visible_items);
        } else {
            app.scroll_offset = 0;
        }
    }

    // Ensure scroll offset is within bounds
    if app.total_list_items > visible_items {
        let max_offset = app.total_list_items.saturating_sub(visible_items);
        app.scroll_offset = app.scroll_offset.min(max_offset);
    } else {
        app.scroll_offset = 0;
    }

    // Create a viewport of the list items based on scroll offset
    let start = app.scroll_offset;
    let end = (start + visible_items).min(list_items.len());
    let visible_list_items: Vec<ListItem> = list_items[start..end].to_vec();

    // Create the list widget with only visible items
    let messages_list = List::new(visible_list_items);

    // Clear the entire padded area to prevent artifacts
    // Create a text block filled with spaces
    let clear_lines: Vec<Line> = (0..padded_area.height)
        .map(|_| Line::from(" ".repeat(padded_area.width as usize)))
        .collect();
    let clear_text = Text::from(clear_lines);
    let clear = Paragraph::new(clear_text);
    f.render_widget(clear, padded_area);

    // Render the list
    f.render_widget(messages_list, padded_area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    // Apply the same padding as the messages area
    let padded_area = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        // .title(if app.is_processing {
        //     " Yapping... "
        // } else {
        //     " Message "
        // })
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

    // Show cursor (hide during processing, loading, MCP connecting, or missing API key)
    if !app.is_processing && !app.is_loading && !app.is_connecting_mcp && !app.missing_api_key {
        let inner_area = input_block.inner(padded_area);
        let cursor_x = inner_area.x + app.cursor_position as u16;
        let cursor_y = inner_area.y;
        f.set_cursor_position((cursor_x.min(inner_area.x + inner_area.width - 1), cursor_y));
    }
}

fn draw_loading_overlay(f: &mut Frame, app: &App, area: Rect) {
    // Calculate popup dimensions - make it centered and reasonably sized
    let popup_width = 60.min(area.width - 4);
    let popup_height = (app.loading_messages.len() as u16 + 6).min(area.height - 4);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area first (draw a background)
    let clear_block = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear_block, popup_area);

    // Create the loading popup with a nice border
    let loading_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Loading Documents ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(Color::White).bg(Color::Black));

    // Create content with loading messages
    let mut lines = vec![];

    // Add a spinner at the top
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_chars[app.spinner_index % spinner_chars.len()];
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            spinner,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Loading..."),
    ]));
    lines.push(Line::from("")); // Empty line for spacing

    // Add all loading messages
    for msg in &app.loading_messages {
        lines.push(Line::from(vec![Span::styled(
            msg.clone(),
            Style::default().fg(Color::Gray),
        )]));
    }

    let paragraph = Paragraph::new(lines)
        .block(loading_block)
        .alignment(ratatui::layout::Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}

fn draw_mcp_connection_overlay(f: &mut Frame, app: &App, area: Rect) {
    // Calculate popup dimensions - make it centered and reasonably sized
    let popup_width = 70.min(area.width - 4);
    let popup_height = 10.min(area.height - 4);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area first (draw a background)
    let clear_block = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear_block, popup_area);

    // Create the MCP connection popup with a nice border
    let connection_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Waiting for MCP Server ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(Color::White).bg(Color::Black));

    // Create content with connection status
    let mut lines = vec![];

    // Add a spinner at the top
    let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_chars[app.spinner_index % spinner_chars.len()];
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            spinner,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            &app.mcp_connection_message,
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from("")); // Empty line for spacing

    // Add helpful information
    lines.push(Line::from(vec![Span::styled(
        (*MCP_SERVER_PORT).clone(),
        Style::default().fg(Color::Gray),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "Run: cargo run -p mcp-server",
        Style::default().fg(Color::Cyan),
    )]));

    let paragraph = Paragraph::new(lines)
        .block(connection_block)
        .alignment(ratatui::layout::Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}

#[allow(clippy::vec_init_then_push)]
fn draw_missing_api_key_overlay(f: &mut Frame, _app: &App, area: Rect) {
    // Calculate popup dimensions - make it a bit larger for the instructions
    let popup_width = 80.min(area.width - 4);
    let popup_height = 12.min(area.height - 4);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear the popup area first (draw a background)
    let clear_block = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(clear_block, popup_area);

    // Create the API key popup with a nice border (red for error)
    let api_key_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red))
        .title(" Anthropic API Key Required ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(Color::White).bg(Color::Black));

    // Create content with instructions
    let mut lines = Vec::new();

    // Add warning icon and message
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "⚠",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "ANTHROPIC_API_KEY environment variable not set",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from("")); // Empty line for spacing

    // Add instructions
    lines.push(Line::from(vec![Span::styled(
        "Please set your Anthropic API key and restart:",
        Style::default().fg(Color::White),
    )]));
    lines.push(Line::from("")); // Empty line for spacing

    lines.push(Line::from(vec![Span::styled(
        "export ANTHROPIC_API_KEY=\"your-api-key-here\"",
        Style::default().fg(Color::Cyan),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "cargo run --bin foameow",
        Style::default().fg(Color::Cyan),
    )]));
    lines.push(Line::from("")); // Empty line for spacing

    lines.push(Line::from(vec![Span::styled(
        "Get your API key at: https://console.anthropic.com",
        Style::default().fg(Color::Gray),
    )]));

    let paragraph = Paragraph::new(lines)
        .block(api_key_block)
        .alignment(ratatui::layout::Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}
