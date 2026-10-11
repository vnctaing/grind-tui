use super::app::{problem_keys, App, Panel, ProblemView};
use crate::catalog::{Body, Difficulty, Problem};
use super::highlight::highlight_code;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub(super) fn draw_problem_page(f: &mut Frame, app: &App, view: &ProblemView) {
    let area = f.area();
    let outer = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).split(area);
    let problem = &app.catalog.problems()[view.idx];
    draw_problem_title(f, outer[0], problem);
    draw_problem_sections(f, outer[1], app, view, problem);
    draw_problem_status(f, outer[2], app, view);
}

fn draw_problem_title(f: &mut Frame, area: Rect, p: &Problem) {
    let title = format!(" {} . {} [{}] ", p.id.0, p.title, difficulty_label(p.difficulty));
    let style = Style::default().fg(difficulty_color(p.difficulty)).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(title).style(style), area);
}

fn draw_problem_sections(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, problem: &Problem) {
    let desc_width = if view.collapsed.desc { Constraint::Length(0) } else { Constraint::Percentage(40) };
    let cols = Layout::default().direction(Direction::Horizontal)
        .constraints([desc_width, Constraint::Min(0)]).split(area);

    if !view.collapsed.desc {
        draw_desc_section(f, cols[0], view, problem);
    }

    // Editor bar is always visible at the top of the right column
    let editor_bar_height = 3u16;
    let output_height = if view.collapsed.output { Constraint::Length(1) } else { Constraint::Percentage(25) };

    if view.collapsed.code {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Length(1), Constraint::Min(0)])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app, view);
        f.render_widget(
            Paragraph::new(Span::styled(
                " [c] ▸ Code preview (hidden) ",
                Style::default().fg(Color::DarkGray),
            )),
            right_rows[1],
        );
        draw_output_section(f, right_rows[2], view, problem);
    } else {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Min(0), output_height])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app, view);
        draw_code_section(f, right_rows[1], app, view);
        draw_output_section(f, right_rows[2], view, problem);
    }
}

fn draw_desc_section(f: &mut Frame, area: Rect, view: &ProblemView, problem: &Problem) {
    let focused = view.focused == Panel::Description;
    let content = match &problem.body {
        Body::Free { description, .. } => description.clone(),
        Body::Premium => format!(
            "Premium problem: the statement is not available offline.\nhttps://leetcode.com/problems/{}/",
            problem.slug,
        ),
    };
    let title = if focused { " Description [focused — p to collapse] " } else { " [p] Description " };
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    f.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false })
            .scroll((view.scroll.desc, 0)),
        area,
    );
}

fn draw_editor_bar(f: &mut Frame, area: Rect, app: &App, view: &ProblemView) {
    let file_path = view.path.display().to_string();

    let line1: Line = if let Some(ref name) = view.detached {
        Line::from(vec![
            Span::styled(" ⟳ ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} ", name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("auto-syncing on save  ", Style::default().fg(Color::DarkGray)),
            Span::styled("[y]", Style::default().fg(Color::Yellow)),
            Span::styled("force sync", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let mut spans = vec![Span::styled(" ✏  Start coding: ", Style::default().fg(Color::DarkGray))];
        for ed in &app.editors {
            spans.push(Span::styled(format!("[{}]", ed.key), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(format!("{} ", ed.name), Style::default().fg(Color::White)));
        }
        Line::from(spans)
    };

    let line2: Line = if file_path.is_empty() {
        Line::from("")
    } else {
        let cmd = view.detached.as_deref()
            .or_else(|| app.editors.first().map(|e| e.command.as_str()))
            .unwrap_or("vim");
        Line::from(vec![
            Span::styled(" 💡 New tab: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} {}", cmd, file_path), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            Span::styled("  [Y]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("yank", Style::default().fg(Color::DarkGray)),
        ])
    };

    f.render_widget(
        Paragraph::new(vec![line1, line2])
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray))),
        area,
    );
}

fn draw_code_section(f: &mut Frame, area: Rect, app: &App, view: &ProblemView) {
    // Code panel is always read-only — never interactive
    let (title, border_style) = if let Some(ref name) = view.detached {
        (
            format!(" ⟳ Live from {} — read-only  [c]hide ", name),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            format!(" Code preview — read-only  [c]hide "),
            Style::default().fg(Color::DarkGray),
        )
    };
    let block = Block::default().borders(Borders::ALL).title(title).border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let highlighted: Vec<Line<'static>> = highlight_code(
        &view.code,
        app.progress.language,
    ).into_iter().enumerate().map(|(i, mut l)| {
        l.spans.insert(0, Span::styled(format!("{:>3} ", i + 1), Style::default().fg(Color::DarkGray)));
        l
    }).collect();

    f.render_widget(
        Paragraph::new(highlighted)
            .wrap(Wrap { trim: false })
            .scroll((view.scroll.code, 0)),
        inner,
    );
}

fn draw_output_section(f: &mut Frame, area: Rect, view: &ProblemView, problem: &Problem) {
    let focused = view.focused == Panel::Output;
    if view.collapsed.output {
        f.render_widget(
            Paragraph::new(Span::styled(" [o] Output ▸ (expand) ", Style::default().fg(Color::DarkGray))),
            area,
        );
        return;
    }
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    let title = if focused { " Output [focused — o to collapse] " } else { " [o] Output " };

    let output = format!("{}\n\nExamples:\n{}", view.message, problem.examples);
    let raw: Vec<&str> = output.lines().collect();
    let lines: Vec<Line> = raw.iter().map(|line| {
        let style = if line.starts_with("Marked solved") {
            Style::default().fg(Color::Green)
        } else if line.starts_with("Examples:") {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(Span::styled(line.to_string(), style))
    }).collect();

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false }).scroll((view.scroll.output, 0)),
        area,
    );
}

fn draw_problem_status(f: &mut Frame, area: Rect, app: &App, view: &ProblemView) {
    let editor_keys: String = app.editors.iter()
        .map(|ed| format!("[{}]{} ", ed.key, ed.name))
        .collect();
    let keys: String = problem_keys(app, view).iter()
        .map(|(k, d)| format!("[{}]{}  ", k, d))
        .collect();
    let status = format!(" {}{}", editor_keys, keys);
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn difficulty_color(d: Difficulty) -> Color {
    match d { Difficulty::Easy => Color::Green, Difficulty::Medium => Color::Yellow, Difficulty::Hard => Color::Red }
}

fn difficulty_label(d: Difficulty) -> &'static str {
    match d { Difficulty::Easy => "Easy", Difficulty::Medium => "Medium", Difficulty::Hard => "Hard" }
}
