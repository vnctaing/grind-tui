use super::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub(super) fn draw_list_page(f: &mut Frame, app: &App, list_state: &mut ListState) {
    let area = f.area();
    let chunks = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)]).split(area);
    draw_list_full(f, chunks[0], app, list_state);
    draw_list_status(f, chunks[1], app);
}

fn draw_list_full(f: &mut Frame, area: Rect, app: &App, list_state: &mut ListState) {
    let rows = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

    let search_text = if app.searching { format!("/{}_", app.search_query) }
        else if !app.search_query.is_empty() { format!("/{}", app.search_query) }
        else { " /  (press / to search)".to_string() };
    f.render_widget(
        Paragraph::new(search_text)
            .style(if app.searching { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
            .block(Block::default().borders(Borders::ALL).title(" Search ")),
        rows[0],
    );

    let items: Vec<ListItem> = app.filtered.iter().map(|&i| {
        let p = &app.problems[i];
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:>4}", p.id), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(if p.solved { "✓" } else { " " }, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::raw(format!("{:<30}", truncate(&p.title, 30))),
            Span::raw("  "),
            Span::styled(format!("{:<8}", p.difficulty.label()), Style::default().fg(p.difficulty.color())),
        ]))
    }).collect();

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL)
                .title(format!(" #    ✓  Title                           Difficulty  │ {} problems ", app.filtered.len())))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("▶ "),
        rows[1], list_state,
    );
}

fn draw_list_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Paragraph::new(format!(
            " [/]search  [Enter]open  [q]quit  │  Problems: {}  Solved: {}",
            app.filtered.len(), app.solved_count,
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
