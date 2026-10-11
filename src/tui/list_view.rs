use super::app::{list_keys, App};
use crate::catalog::Difficulty;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub(super) fn draw_list_page(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)]).split(area);
    draw_list_full(f, chunks[0], app);
    draw_list_status(f, chunks[1], app);
}

fn draw_list_full(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

    let search_text = if app.searching { format!("/{}_", app.query) }
        else if !app.query.is_empty() { format!("/{}", app.query) }
        else { " /  (press / to search)".to_string() };
    f.render_widget(
        Paragraph::new(search_text)
            .style(if app.searching { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
            .block(Block::default().borders(Borders::ALL).title(" Search ")),
        rows[0],
    );

    let items: Vec<ListItem> = app.filtered.iter().map(|&i| {
        let p = &app.catalog.problems()[i];
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:>4}", p.id.0), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(if app.is_solved(i) { "✓" } else { " " }, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::raw(format!("{:<30}", truncate(&p.title, 30))),
            Span::raw("  "),
            Span::styled(format!("{:<8}", difficulty_label(p.difficulty)), Style::default().fg(difficulty_color(p.difficulty))),
        ]))
    }).collect();

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL)
                .title(format!(" #    ✓  Title                           Difficulty  │ {} problems ", app.filtered.len())))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ "),
        rows[1], &mut app.list_state,
    );
}

fn draw_list_status(f: &mut Frame, area: Rect, app: &App) {
    let keys: String = list_keys(app).iter().map(|(k, d)| format!("[{}]{}  ", k, d)).collect();
    f.render_widget(
        Paragraph::new(format!(
            " {} │  Problems: {}  Solved: {}",
            keys, app.filtered.len(), app.solved_count(),
        )).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else {
        let cut = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(max);
        format!("{}…", &s[..cut])
    }
}

fn difficulty_color(d: Difficulty) -> Color {
    match d { Difficulty::Easy => Color::Green, Difficulty::Medium => Color::Yellow, Difficulty::Hard => Color::Red }
}

fn difficulty_label(d: Difficulty) -> &'static str {
    match d { Difficulty::Easy => "Easy", Difficulty::Medium => "Medium", Difficulty::Hard => "Hard" }
}
