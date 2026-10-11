use super::app::{list_keys, App, ListFilter};
use super::theme::{self, t};
use crate::catalog::ListId;
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

const TABS: [ListFilter; 3] = [
    ListFilter::All,
    ListFilter::Only(ListId::Grind75),
    ListFilter::Only(ListId::NeetCode150),
];

const GAP: &str = "  ";
const MARKER_W: usize = 2;
const MARK_W: usize = 1;
const BADGE_W: usize = 2;
const DIFFICULTY_W: usize = 10;

pub(super) fn draw_list_page(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(t::BG)), area);
    let area = area.inner(Margin { horizontal: 1, vertical: 0 });
    let [header, _, tabs, search, table, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, header, app);
    draw_tabs(f, tabs, app);
    draw_search(f, search, app);
    draw_table(f, table, app);
    f.render_widget(Paragraph::new(theme::help_line(&list_keys(app))), help);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let flank = Style::default().fg(t::PURPLE);
    let mut left = vec![Span::styled("░▒▓ ", flank)];
    left.extend(theme::gradient(theme::BRAND, t::MAGENTA, t::CYAN));
    left.push(Span::styled(" ▓▒░", flank));
    let right = theme::progress_bar(app.solved_count(), app.catalog.problems().len(), 16);
    let line = justify(left, right, usize::from(area.width));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for (i, tab) in TABS.iter().copied().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" • ", Style::default().fg(t::BORDER)));
        }
        let label = tab.label();
        spans.push(if tab == app.filter {
            theme::pill(label, t::BG, t::MAGENTA)
        } else {
            Span::styled(format!(" {label} "), Style::default().fg(t::MUTED))
        });
    }
    let count = vec![Span::styled(
        format!("{} problems", app.filtered.len()),
        Style::default().fg(t::MUTED),
    )];
    let line = justify(spans, count, usize::from(area.width));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_search(f: &mut Frame, area: Rect, app: &App) {
    let (border, title) = if app.searching {
        (t::CYAN, theme::pill("search", t::BG, t::CYAN))
    } else {
        (
            t::BORDER,
            Span::styled(
                " search ",
                Style::default().fg(t::MUTED).add_modifier(Modifier::BOLD),
            ),
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(t::BG))
        .title(Line::from(title));
    let line = if app.searching {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(t::CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(app.query.clone(), Style::default().fg(t::FG)),
            Span::styled("▏", Style::default().fg(t::CYAN)),
        ])
    } else if app.query.is_empty() {
        Line::from(Span::styled(
            "/ search by title or id",
            Style::default().fg(t::MUTED),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(t::MUTED)),
            Span::styled(app.query.clone(), Style::default().fg(t::FG)),
        ])
    };
    f.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_table(f: &mut Frame, area: Rect, app: &mut App) {
    let hint = if app.query.is_empty() {
        String::new()
    } else {
        format!("{} matches", app.filtered.len())
    };
    let block = theme::panel("problems", &hint, !app.searching);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [head, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let [list_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(body);
    let id_w = app
        .catalog
        .problems()
        .iter()
        .map(|p| p.id.0.to_string().len())
        .max()
        .unwrap_or(1);
    let row_w = usize::from(list_area.width);
    let title_w = row_w.saturating_sub(MARKER_W + id_w + MARK_W + BADGE_W + DIFFICULTY_W + 4 * GAP.len());
    let muted = Style::default().fg(t::MUTED);

    let head_line = row(
        vec![
            Span::raw(" ".repeat(MARKER_W)),
            Span::styled(fit("#", id_w, true), muted),
            Span::raw(GAP),
            Span::raw(" ".repeat(MARK_W)),
            Span::raw(GAP),
            Span::styled(fit("Title", title_w, false), muted),
            Span::raw(GAP),
            Span::raw(" ".repeat(BADGE_W)),
            Span::raw(GAP),
            Span::styled(fit("Difficulty", DIFFICULTY_W, false), muted),
        ],
        row_w,
        None,
    );
    f.render_widget(Paragraph::new(head_line), head);

    if app.filtered.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  no problems match", muted)),
            list_area,
        );
        return;
    }

    let selected = app.list_state.selected();
    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .map(|(n, &idx)| {
            let p = &app.catalog.problems()[idx];
            let sel = Some(n) == selected;
            let bg = sel.then_some(t::SELECTED_BG);

            let marker = if sel { "▌ " } else { "  " };
            let id_style = if sel {
                Style::default().fg(t::FG).add_modifier(Modifier::BOLD)
            } else {
                muted
            };
            let mark = if app.is_solved(idx) {
                Span::styled("✓", Style::default().fg(t::MINT))
            } else {
                Span::styled("·", Style::default().fg(t::BORDER))
            };
            let mut badges = Vec::new();
            if app.catalog.in_list(idx, ListId::Grind75) {
                badges.push(Span::styled("G", Style::default().fg(t::CYAN)));
            }
            if app.catalog.in_list(idx, ListId::NeetCode150) {
                badges.push(Span::styled("N", Style::default().fg(t::PURPLE)));
            }
            let badge_pad = BADGE_W.saturating_sub(badges.len());
            badges.push(Span::raw(" ".repeat(badge_pad)));

            let mut spans = vec![
                Span::styled(marker, Style::default().fg(t::MAGENTA)),
                Span::styled(fit(&p.id.0.to_string(), id_w, true), id_style),
                Span::raw(GAP),
                mark,
                Span::raw(GAP),
                Span::styled(
                    fit(&p.title, title_w, false),
                    Style::default().fg(t::FG).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::raw(GAP),
            ];
            spans.extend(badges);
            spans.push(Span::raw(GAP));
            spans.push(Span::styled(
                fit(theme::difficulty_label(p.difficulty), DIFFICULTY_W, false),
                Style::default().fg(theme::difficulty_color(p.difficulty)),
            ));
            ListItem::new(row(spans, row_w, bg))
        })
        .collect();

    f.render_stateful_widget(List::new(items), list_area, &mut app.list_state);

    let visible = usize::from(list_area.height);
    let total = app.filtered.len();
    if total > visible {
        let mut state = ScrollbarState::new(total)
            .position(app.list_state.offset())
            .viewport_content_length(visible);
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(Style::default().fg(t::BORDER))
            .thumb_symbol("█")
            .thumb_style(Style::default().fg(t::MAGENTA));
        f.render_stateful_widget(bar, bar_area, &mut state);
    }
}

fn justify(mut left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let used = spans_width(&left) + spans_width(&right);
    left.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    left.extend(right);
    Line::from(left)
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(Span::width).sum()
}

fn row(spans: Vec<Span<'static>>, width: usize, bg: Option<Color>) -> Line<'static> {
    let used = spans_width(&spans);
    let mut spans: Vec<Span<'static>> = match bg {
        Some(c) => spans
            .into_iter()
            .map(|s| s.patch_style(Style::default().bg(c)))
            .collect(),
        None => spans,
    };
    let pad = " ".repeat(width.saturating_sub(used));
    spans.push(match bg {
        Some(c) => Span::styled(pad, Style::default().bg(c)),
        None => Span::raw(pad),
    });
    Line::from(spans)
}

fn fit(s: &str, width: usize, right: bool) -> String {
    let text: String = if s.chars().count() <= width {
        s.to_string()
    } else if width == 0 {
        String::new()
    } else {
        s.chars().take(width - 1).chain(std::iter::once('…')).collect()
    };
    let pad = " ".repeat(width.saturating_sub(text.chars().count()));
    if right { pad + &text } else { text + &pad }
}
