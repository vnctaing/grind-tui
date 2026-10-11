use super::buffer::char_cells;
use super::{CodeEditor, Viewport};
use crate::tui::highlight::highlight_code;
use crate::tui::theme::{self, t};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::ops::Range;

pub(super) fn render(ed: &CodeEditor, area: Rect, out: &mut ratatui::buffer::Buffer, show_cursor: bool) {
    let buf = &ed.buffer;
    let digits = buf.line_count().to_string().len();
    let [gutter, text] = Layout::horizontal([
        Constraint::Length(digits as u16 + 3),
        Constraint::Min(0),
    ])
    .areas(area);
    ed.viewport.set(Viewport { rows: usize::from(text.height), cols: usize::from(text.width) });

    let head = ed.cursor.sel.head;
    let caret_line = buf.pos(head).line;
    let caret = show_cursor.then_some(head);
    let selection = ed.cursor.sel.range();
    let highlighted = highlight_code(buf.text(), ed.lang);
    let top = ed.scroll.top.min(buf.line_count() - 1);
    let rows = top..(top + usize::from(text.height)).min(buf.line_count());

    let numbers: Vec<Line> = rows
        .clone()
        .map(|i| {
            let fg = if i == caret_line { t::FG } else { t::MUTED };
            Line::from(vec![
                Span::styled(format!("{:>digits$} ", i + 1), Style::default().fg(fg)),
                Span::styled("│ ", Style::default().fg(t::BORDER)),
            ])
        })
        .collect();
    let lines: Vec<Line> = rows
        .map(|i| {
            let base = highlighted.get(i).map_or(&[][..], |l| &l.spans[..]);
            line_spans(buf.line_range(i), buf.text(), base, &selection, caret)
        })
        .collect();

    Paragraph::new(numbers).render(gutter, out);
    let left = u16::try_from(ed.scroll.left).unwrap_or(u16::MAX);
    Paragraph::new(lines).scroll((0, left)).render(text, out);
}

/// One buffer line as spans: syntax color per char, then selection, then the caret.
fn line_spans(
    range: Range<usize>,
    text: &str,
    base: &[Span<'static>],
    selection: &Range<usize>,
    caret: Option<usize>,
) -> Line<'static> {
    let mut base_styles = base
        .iter()
        .flat_map(|s| s.content.chars().map(move |_| s.style));
    let style_at = |off: usize, style: Style| {
        let style = if selection.contains(&off) { style.patch(theme::editor_selection()) } else { style };
        if caret == Some(off) { style.patch(theme::editor_cursor()) } else { style }
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut col = 0;
    for (i, c) in text[range.clone()].char_indices() {
        let base = base_styles.next().unwrap_or(Style::default().fg(t::FG));
        let cells = char_cells(c, col);
        let shown = if c == '\t' { " ".repeat(cells) } else { c.to_string() };
        col += cells;
        push(&mut spans, shown, style_at(range.start + i, base));
    }
    let end = range.end;
    let newline_selected = end < text.len() && selection.contains(&end);
    if caret == Some(end) || newline_selected {
        push(&mut spans, " ".to_string(), style_at(end, Style::default().fg(t::FG)));
    }
    Line::from(spans)
}

fn push(spans: &mut Vec<Span<'static>>, text: String, style: Style) {
    match spans.last_mut() {
        Some(last) if last.style == style => last.content.to_mut().push_str(&text),
        _ => spans.push(Span::styled(text, style)),
    }
}
