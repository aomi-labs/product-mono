use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{app::App, messages::draw_messages};

pub const MAX_WIDTH: u16 = 120;

pub fn draw(f: &mut Frame, app: &mut App) {
    let mut full_area = f.area();
    full_area.height = full_area.height.saturating_sub(2);

    let area = center_area(full_area, MAX_WIDTH);
    let outer_block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
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

pub(super) fn draw_input(f: &mut Frame, app: &App, area: Rect) {
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
            ratatui::style::Color::Cyan
        } else {
            ratatui::style::Color::White
        }));

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(ratatui::style::Color::White))
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
