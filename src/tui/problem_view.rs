use super::app::{App, Panel};
use super::editor::code_file_path;
use super::highlight::highlight_code;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub(super) fn draw_problem_page(f: &mut Frame, app: &App) {
    let area = f.area();
    let outer = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).split(area);
    draw_problem_title(f, outer[0], app);
    draw_problem_sections(f, outer[1], app);
    draw_problem_status(f, outer[2], app);
}

fn draw_problem_title(f: &mut Frame, area: Rect, app: &App) {
    let title = app.selected_problem()
        .map(|p| format!(" {} . {} [{}] ", p.id, p.title, p.difficulty.label()))
        .unwrap_or_else(|| " Problem ".to_string());
    let style = app.selected_problem()
        .map(|p| Style::default().fg(p.difficulty.color()).add_modifier(Modifier::BOLD))
        .unwrap_or_else(|| Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(Paragraph::new(title).style(style), area);
}

fn draw_problem_sections(f: &mut Frame, area: Rect, app: &App) {
    let desc_width = if app.desc_collapsed { Constraint::Length(0) } else { Constraint::Percentage(40) };
    let cols = Layout::default().direction(Direction::Horizontal)
        .constraints([desc_width, Constraint::Min(0)]).split(area);

    if !app.desc_collapsed {
        draw_desc_section(f, cols[0], app, app.focused_panel == Panel::Description);
    }

    // Editor bar is always visible at the top of the right column
    let editor_bar_height = 3u16;
    let output_height = if app.output_collapsed { Constraint::Length(1) } else { Constraint::Percentage(25) };

    if app.code_collapsed {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Length(1), Constraint::Min(0)])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app);
        f.render_widget(
            Paragraph::new(Span::styled(
                " [c] ▸ Code preview (hidden) ",
                Style::default().fg(Color::DarkGray),
            )),
            right_rows[1],
        );
        draw_output_section(f, right_rows[2], app, app.focused_panel == Panel::Output);
    } else {
        let right_rows = Layout::default().direction(Direction::Vertical)
            .constraints([Constraint::Length(editor_bar_height), Constraint::Min(0), output_height])
            .split(cols[1]);
        draw_editor_bar(f, right_rows[0], app);
        draw_code_section(f, right_rows[1], app, app.focused_panel == Panel::Code);
        draw_output_section(f, right_rows[2], app, app.focused_panel == Panel::Output);
    }
}

fn draw_desc_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    let content = app.selected_problem().map(|p| p.description.as_str())
        .unwrap_or("Select a problem and press Enter to load it.");
    let title = if focused { " Description [focused — p to collapse] " } else { " [p] Description " };
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    f.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false })
            .scroll((app.desc_scroll, 0)),
        area,
    );
}

fn draw_editor_bar(f: &mut Frame, area: Rect, app: &App) {
    let file_path = app.selected_problem()
        .map(|p| code_file_path(&p.slug, &app.language))
        .unwrap_or_default();

    let line1: Line = if let Some(ref name) = app.detached_editor_name {
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
        let cmd = app.detached_editor_name.as_deref()
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

fn draw_code_section(f: &mut Frame, area: Rect, app: &App, _focused: bool) {
    // Code panel is always read-only — never interactive
    let (title, border_style) = if let Some(ref name) = app.detached_editor_name {
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
        app.selected_problem().map(|p| p.code.as_str()).unwrap_or(""),
        crate::catalog::Lang::ALL.into_iter().find(|l| l.slug() == app.language).unwrap_or_default(),
    ).into_iter().enumerate().map(|(i, mut l)| {
        l.spans.insert(0, Span::styled(format!("{:>3} ", i + 1), Style::default().fg(Color::DarkGray)));
        l
    }).collect();

    f.render_widget(
        Paragraph::new(highlighted)
            .wrap(Wrap { trim: false })
            .scroll((app.code_scroll, 0)),
        inner,
    );
}

fn diff_line(got: &str, expected: &str, label: &str, base_color: Color) -> Line<'static> {
    let parse = |s: &str| -> Option<Vec<String>> {
        let t = s.trim();
        if t.starts_with('[') && t.ends_with(']') {
            Some(t[1..t.len()-1].split(',').map(|x| x.trim().to_string()).collect())
        } else {
            None
        }
    };

    let dim = Style::default().fg(Color::DarkGray);
    let highlight = Style::default().fg(base_color).add_modifier(Modifier::BOLD);
    let normal = Style::default().fg(base_color);

    let other = if label == "Got:     " { expected } else { got };

    if let (Some(items), Some(other_items)) = (parse(s_for(label, got, expected)), parse(other)) {
        let mut spans = vec![Span::styled(format!("{} [", label), normal)];
        for (i, item) in items.iter().enumerate() {
            let matches = other_items.get(i).map(|o| o == item).unwrap_or(false);
            spans.push(Span::styled(item.clone(), if matches { dim } else { highlight }));
            if i + 1 < items.len() {
                spans.push(Span::styled(",", normal));
            }
        }
        // extra items not in other
        for i in items.len()..other_items.len() {
            let _ = i; // missing items — already shown as dim absence
        }
        spans.push(Span::styled("]", normal));
        Line::from(spans)
    } else {
        Line::from(Span::styled(format!("{} {}", label, s_for(label, got, expected)), normal))
    }
}

fn s_for<'a>(label: &str, got: &'a str, expected: &'a str) -> &'a str {
    if label.starts_with("Got") { got } else { expected }
}

fn draw_output_section(f: &mut Frame, area: Rect, app: &App, focused: bool) {
    if app.output_collapsed {
        f.render_widget(
            Paragraph::new(Span::styled(" [o] Output ▸ (expand) ", Style::default().fg(Color::DarkGray))),
            area,
        );
        return;
    }
    let border_style = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    let title = if focused { " Output [focused — o to collapse] " } else { " [o] Output " };

    let raw: Vec<&str> = app.output.lines().collect();
    let mut lines: Vec<Line> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let line = raw[i];
        // Detect Got/Expected pair and do element-level diff
        if line.starts_with("Got:") && i + 1 < raw.len() && raw[i+1].starts_with("Expected:") {
            let got_val = line.trim_start_matches("Got:").trim();
            let exp_val = raw[i+1].trim_start_matches("Expected:").trim();
            lines.push(diff_line(got_val, exp_val, "Got:     ", Color::Red));
            lines.push(diff_line(got_val, exp_val, "Expected:", Color::Green));
            i += 2;
            continue;
        }
        let style = if line.starts_with("✓") || line.starts_with("Status: Accepted") {
            Style::default().fg(Color::Green)
        } else if line.starts_with("✗") || line.starts_with("Status: Wrong") || line.starts_with("Status: Runtime") || line.starts_with("Status: Time") {
            Style::default().fg(Color::Red)
        } else if line.starts_with("Input:") {
            Style::default().fg(Color::Yellow)
        } else if line.starts_with("Status:") {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(line.to_string(), style)));
        i += 1;
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style))
            .wrap(Wrap { trim: false }).scroll((app.output_scroll, 0)),
        area,
    );
}

fn draw_problem_status(f: &mut Frame, area: Rect, app: &App) {
    let editor_keys: String = app.editors.iter()
        .map(|ed| format!("[{}]{} ", ed.key, ed.name))
        .collect();
    let code_label = if app.code_collapsed { "[c]show code" } else { "[c]hide code" };
    let status = format!(
        " [b]back  {}[r]run  [s]submit  [l]lang:{}  [p]desc  [o]output  {}  [Tab]p↔o  [q]quit",
        editor_keys, app.language, code_label,
    );
    f.render_widget(
        Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}
