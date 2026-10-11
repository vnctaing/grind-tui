use super::app::{problem_keys, App, Panel, ProblemView};
use super::highlight::{highlight_code, with_gutter};
use super::markdown::render_markdown;
use super::theme::{self, help_line, panel, t};
use crate::catalog::{Body, Problem};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

pub(super) fn draw_problem_page(f: &mut Frame, app: &App, view: &ProblemView) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(t::BG)), area);
    let problem = &app.catalog.problems()[view.idx];
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    draw_top_bar(f, rows[0], app, view, problem);
    draw_body(f, rows[1], app, view, problem);
    f.render_widget(Paragraph::new(help_line(&problem_keys(app, view))), rows[2]);
}

fn draw_top_bar(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, p: &Problem) {
    let mut spans = vec![
        theme::pill(&p.id.0.to_string(), t::BG, t::CYAN),
        Span::styled(
            format!(" {}", p.title),
            Style::default().fg(t::FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        theme::pill(
            theme::difficulty_label(p.difficulty),
            t::BG,
            theme::difficulty_color(p.difficulty),
        ),
        Span::raw(" "),
        theme::pill(app.progress.language.label(), t::BG, t::PURPLE),
    ];
    if matches!(p.body, Body::Premium) {
        spans.push(Span::raw(" "));
        spans.push(theme::pill("PREMIUM", t::BG, t::YELLOW));
    }
    spans.push(Span::raw(" "));
    spans.push(status_pill(app.is_solved(view.idx)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, problem: &Problem) {
    let right = if view.collapsed.desc {
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
        draw_collapsed(f, rows[0], "Description", "p");
        rows[1]
    } else {
        let cols = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
        draw_description(f, cols[0], view, problem);
        cols[2]
    };
    draw_right(f, right, app, view, problem);
}

fn draw_right(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, problem: &Problem) {
    let code_h = if view.collapsed.code { Constraint::Length(1) } else { Constraint::Min(0) };
    let status_h = if view.collapsed.output { Constraint::Length(1) } else { Constraint::Percentage(35) };
    let rows = Layout::vertical([Constraint::Length(2), code_h, status_h]).split(area);

    draw_editor_bar(f, rows[0], app, view);
    if view.collapsed.code {
        draw_collapsed(f, rows[1], "Code", "c");
    } else {
        draw_code(f, rows[1], app, view);
    }
    if view.collapsed.output {
        draw_collapsed(f, rows[2], "Status", "o");
    } else {
        draw_status(f, rows[2], app, view, problem);
    }
}

fn draw_collapsed(f: &mut Frame, area: Rect, label: &str, key: &str) {
    let text = format!(" ▸ {label}  {key} to expand");
    f.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(t::MUTED))),
        area,
    );
}

fn draw_description(f: &mut Frame, area: Rect, view: &ProblemView, problem: &Problem) {
    let lines = match &problem.body {
        Body::Free { description, .. } => render_markdown(description),
        Body::Premium => premium_lines(problem),
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Description", "p hide", view.focused == Panel::Description))
            .wrap(Wrap { trim: false })
            .scroll((view.scroll.desc, 0)),
        area,
    );
}

fn premium_lines(p: &Problem) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Premium problem. The statement is not in the offline catalog.",
            Style::default().fg(t::MUTED),
        )),
        Line::from(Span::styled(
            format!("https://leetcode.com/problems/{}/", p.slug),
            Style::default().fg(t::CYAN).add_modifier(Modifier::UNDERLINED),
        )),
        Line::default(),
    ];
    lines.extend(examples_block(&p.examples));
    lines
}

fn examples_block(examples: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Examples",
        Style::default().fg(t::CYAN).add_modifier(Modifier::BOLD),
    ))];
    lines.extend(examples.lines().map(|l| {
        Line::from(Span::styled(l.to_string(), Style::default().fg(t::FG)))
    }));
    lines
}

fn draw_editor_bar(f: &mut Frame, area: Rect, app: &App, view: &ProblemView) {
    let command = app.editors.first().map_or("vim", |e| e.command.as_str());
    let path = match view.path.file_name() {
        Some(name) => format!("~/.grind-tui/{}", name.to_string_lossy()),
        None => view.path.display().to_string(),
    };
    let (first, second) = match &view.detached {
        Some(name) => (
            Line::from(Span::styled(
                format!("↻ {name} auto-syncs on save"),
                Style::default().fg(t::MINT),
            )),
            Line::from(Span::styled(path, Style::default().fg(t::MUTED))),
        ),
        None => {
            let keys: Vec<(&str, &str)> = app
                .editors
                .iter()
                .map(|ed| (ed.key.as_str(), ed.name.as_str()))
                .collect();
            let mut spans = vec![Span::styled("open in  ", Style::default().fg(t::MUTED))];
            spans.extend(help_line(&keys).spans);
            (
                Line::from(spans),
                Line::from(Span::styled(
                    format!("{command} {path}"),
                    Style::default().fg(t::MUTED),
                )),
            )
        }
    };
    f.render_widget(Paragraph::new(vec![first, second]), area);
}

fn draw_code(f: &mut Frame, area: Rect, app: &App, view: &ProblemView) {
    let hint = match &view.detached {
        Some(name) => format!("live from {name} · c hide"),
        None => "read-only · c hide".to_string(),
    };
    let lines = with_gutter(highlight_code(&view.code, app.progress.language));
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Code", &hint, view.focused == Panel::Code))
            .scroll((view.scroll.code, 0)),
        area,
    );
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, problem: &Problem) {
    let mut lines = vec![Line::from(status_pill(app.is_solved(view.idx)))];
    lines.extend(
        view.message
            .lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(t::FG)))),
    );
    lines.push(Line::default());
    lines.extend(examples_block(&problem.examples));
    f.render_widget(
        Paragraph::new(lines)
            .block(panel("Status", "o hide", view.focused == Panel::Output))
            .wrap(Wrap { trim: false })
            .scroll((view.scroll.output, 0)),
        area,
    );
}

fn status_pill(solved: bool) -> Span<'static> {
    if solved {
        theme::pill("SOLVED", t::BG, t::MINT)
    } else {
        theme::pill("TODO", t::FG, t::BORDER)
    }
}
