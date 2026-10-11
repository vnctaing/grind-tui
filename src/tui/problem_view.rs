use super::app::{problem_keys, App, CodeMode, Panel, ProblemView};
use super::markdown::render_markdown;
use super::theme::{self, help_line, panel, t};
use crate::catalog::{Body, Problem};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

pub(super) fn draw_problem_page(f: &mut Frame, app: &App, view: &ProblemView) {
    let area = f.area();
    let problem = &app.catalog.problems()[view.idx];
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    draw_top_bar(f, rows[0], app, view, problem);
    draw_body(f, rows[1], app, view, problem);
    f.render_widget(Paragraph::new(fitted_help_line(&problem_keys(app, view), rows[2].width)), rows[2]);
}

fn fitted_help_line(items: &[(&str, &str)], width: u16) -> Line<'static> {
    let fits = (0..=items.len())
        .rev()
        .find(|&n| help_line(&items[..n]).width() <= usize::from(width))
        .unwrap_or(0);
    help_line(&items[..fits])
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
        draw_code(f, rows[1], view);
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

fn draw_code(f: &mut Frame, area: Rect, view: &ProblemView) {
    let editing = view.mode == CodeMode::Edit;
    let dirty = if view.editor.is_dirty() { "● unsaved · " } else { "" };
    let hint = match (editing, &view.detached) {
        (true, _) => {
            let (line, col) = view.editor.caret();
            format!("{dirty}Ln {line}, Col {col} · Esc done")
        }
        (false, Some(name)) => format!("{dirty}live from {name} · Enter edit"),
        (false, None) => format!("{dirty}Enter edit · c hide"),
    };
    let block = panel("Code", &hint, view.focused == Panel::Code || editing);
    let inner = block.inner(area);
    f.render_widget(block, area);
    view.editor.render(inner, f.buffer_mut(), editing);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, view: &ProblemView, problem: &Problem) {
    let mut lines = vec![Line::from(status_pill(app.is_solved(view.idx)))];
    lines.extend(
        view.message
            .lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(t::FG)))),
    );
    if !matches!(problem.body, Body::Premium) {
        lines.push(Line::default());
        lines.extend(examples_block(&problem.examples));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::state::AppState;
    use crate::tui::app::{Collapsed, ListFilter, Page};
    use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};

    fn app_on(slug: &str) -> App {
        let mut app = App::new(Catalog::embedded(), AppState::default(), vec![], ListFilter::All, None);
        let idx = app.catalog.problems().iter().position(|p| p.slug == slug).unwrap();
        let code = app.catalog.problems()[idx].starter(app.progress.language).to_string();
        let path = std::env::temp_dir().join("grind-tui-problem-view-test").join(format!("{slug}.py"));
        app.page = Page::Problem(ProblemView::new(idx, path, &code, app.progress.language));
        app
    }

    fn view_mut(app: &mut App) -> &mut ProblemView {
        match &mut app.page {
            Page::Problem(view) => view,
            Page::List => unreachable!(),
        }
    }

    fn render(app: &mut App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| super::super::draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    fn text(buf: &Buffer) -> String {
        rows(buf).join("\n")
    }

    fn border_fg(buf: &Buffer, title: &str) -> Color {
        let needle: Vec<char> = format!("╭ {title} ").chars().collect();
        for y in 0..buf.area.height {
            let row: Vec<char> = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect();
            if let Some(x) = row.windows(needle.len()).position(|w| w == needle.as_slice()) {
                return buf[(x as u16, y)].fg;
            }
        }
        panic!("no {title} panel");
    }

    #[test]
    fn free_description_renders_markdown_without_raw_markup() {
        let mut app = app_on("two-sum");
        let screen = text(&render(&mut app, 140, 60));
        assert!(screen.contains("Example 1:"));
        for raw in ["&lt;", "&#39;", "**", "`"] {
            assert!(!screen.contains(raw), "raw {raw:?} leaked");
        }
    }

    #[test]
    fn premium_problem_shows_badge_link_and_examples_once() {
        let mut app = app_on("meeting-rooms");
        let screen = text(&render(&mut app, 140, 40));
        assert!(screen.contains("PREMIUM"));
        assert!(screen.contains("https://leetcode.com/problems/meeting-rooms/"));
        assert_eq!(screen.matches("[[0,30],[5,10],[15,20]]").count(), 1);
    }

    #[test]
    fn status_pill_flips_after_toggle_solved() {
        let mut app = app_on("two-sum");
        let before = text(&render(&mut app, 140, 40));
        assert!(before.contains("TODO") && !before.contains("SOLVED"));
        app.toggle_solved();
        let after = text(&render(&mut app, 140, 40));
        assert!(after.contains("SOLVED") && !after.contains("TODO"));
        app.toggle_solved();
        let again = text(&render(&mut app, 140, 40));
        assert!(again.contains("TODO") && !again.contains("SOLVED"));
    }

    #[test]
    fn only_the_focused_panel_border_is_magenta() {
        let mut app = app_on("two-sum");
        for focus in [Panel::Code, Panel::Description, Panel::Output] {
            view_mut(&mut app).focused = focus;
            let buf = render(&mut app, 140, 40);
            for (panel, title) in [(Panel::Description, "Description"), (Panel::Code, "Code"), (Panel::Output, "Status")] {
                let expected = if panel == focus { t::MAGENTA } else { t::BORDER };
                assert_eq!(border_fg(&buf, title), expected, "{title} with {focus:?} focused");
            }
        }
    }

    fn find(buf: &Buffer, glyph: &str, from: (u16, u16), step: (u16, u16)) -> (u16, u16) {
        let (mut x, mut y) = from;
        while buf[(x, y)].symbol() != glyph {
            x += step.0;
            y += step.1;
        }
        (x, y)
    }

    fn description_rect(buf: &Buffer) -> (u16, u16, u16, u16) {
        let (x0, y0) = find(buf, "╭", (0, 0), (0, 1));
        let (x1, _) = find(buf, "╮", (x0 + 1, y0), (1, 0));
        let (_, y1) = find(buf, "╰", (x0, y0 + 1), (0, 1));
        (x0, y0, x1, y1)
    }

    #[test]
    fn every_cell_has_the_theme_background() {
        let mut app = app_on("invert-binary-tree");
        let buf = render(&mut app, 140, 40);
        for cell in buf.content() {
            assert_ne!(cell.bg, Color::Reset, "terminal default bg shows through");
        }
        let (_, y0, x1, y1) = description_rect(&buf);
        for y in y0..=y1 {
            let gap = &buf[(x1 + 1, y)];
            assert_eq!((gap.fg, gap.bg), (t::FG, t::BG), "gap cell at row {y}");
        }
    }

    #[test]
    fn unfocused_description_border_is_border_colored_on_every_edge() {
        let mut app = app_on("invert-binary-tree");
        let buf = render(&mut app, 140, 40);
        let (x0, y0, x1, y1) = description_rect(&buf);
        let edges = (y0 + 1..y1)
            .flat_map(|y| [(x0, y), (x1, y)])
            .chain((x0..=x1).map(|x| (x, y1)));
        for (x, y) in edges {
            assert_eq!(buf[(x, y)].fg, t::BORDER, "border cell ({x}, {y}) {:?}", buf[(x, y)].symbol());
        }
    }

    #[test]
    fn code_panel_is_editable_and_shows_the_caret_only_in_edit_mode() {
        let mut app = app_on("two-sum");
        let nav = render(&mut app, 140, 40);
        let screen = text(&nav);
        assert!(!screen.contains("read-only"));
        assert!(screen.contains("Enter edit"));
        let cyan = |buf: &Buffer| buf.content().iter().filter(|c| c.bg == t::CYAN).count();

        view_mut(&mut app).mode = CodeMode::Edit;
        let edit = render(&mut app, 140, 40);
        assert!(text(&edit).contains("Ln 1, Col 1 · Esc done"));
        assert_eq!(cyan(&edit), cyan(&nav) + 1);
    }

    #[test]
    fn quit_is_visible_at_80_cols() {
        let mut app = app_on("two-sum");
        let bottom = rows(&render(&mut app, 80, 24)).pop().unwrap();
        assert!(bottom.contains("q quit"), "{bottom:?}");
    }

    #[test]
    fn help_bar_never_cuts_an_item() {
        let mut app = app_on("two-sum");
        for width in [20, 40, 60, 80, 100, 140] {
            let buf = render(&mut app, width, 24);
            let bottom = rows(&buf).pop().unwrap();
            let bottom = bottom.trim_end();
            let Page::Problem(view) = &app.page else { unreachable!() };
            let keys = problem_keys(&app, view);
            assert!(
                bottom.is_empty() || keys.iter().any(|(_, desc)| bottom.ends_with(desc)),
                "help bar cut at width {width}: {bottom:?}"
            );
        }
    }

    #[test]
    fn tiny_and_common_sizes_never_panic() {
        let mut sizes: Vec<(u16, u16)> = (0..=12).flat_map(|w| (0..=14).map(move |h| (w, h))).collect();
        sizes.extend([(80, 24), (140, 40)]);
        for slug in ["two-sum", "meeting-rooms"] {
            let mut app = app_on(slug);
            for mask in 0..16u8 {
                view_mut(&mut app).collapsed = Collapsed {
                    desc: mask & 1 != 0,
                    code: mask & 2 != 0,
                    output: mask & 4 != 0,
                };
                view_mut(&mut app).mode = if mask & 8 != 0 { CodeMode::Edit } else { CodeMode::Navigate };
                for &(w, h) in &sizes {
                    render(&mut app, w, h);
                }
            }
        }
    }
}
