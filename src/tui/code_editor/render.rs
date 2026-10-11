use super::buffer::{char_cells, Buffer};
use super::{CodeEditor, Diagnostic, Severity, Viewport};
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
    let (area, status) = if area.height >= 3 {
        let [code, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        (code, Some(status))
    } else {
        (area, None)
    };
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
    let marks = marks(buf, &ed.diagnostics);
    let highlighted = highlight_code(buf.text(), ed.lang);
    let top = ed.scroll.top.min(buf.line_count() - 1);
    let rows = top..(top + usize::from(text.height)).min(buf.line_count());

    let numbers: Vec<Line> = rows
        .clone()
        .map(|i| {
            let fg = if i == caret_line { t::FG } else { t::MUTED };
            let worst = ed
                .diagnostics
                .iter()
                .filter(|d| buf.pos(d.range.start).line == i)
                .map(|d| d.severity)
                .min();
            Line::from(vec![
                Span::styled(format!("{:>digits$}", i + 1), Style::default().fg(fg)),
                gutter_marker(worst),
                Span::styled("│ ", Style::default().fg(t::BORDER)),
            ])
        })
        .collect();
    let lines: Vec<Line> = rows
        .map(|i| {
            let base = highlighted.get(i).map_or(&[][..], |l| &l.spans[..]);
            line_spans(buf.line_range(i), buf.text(), base, &marks, &selection, caret)
        })
        .collect();

    Paragraph::new(numbers).render(gutter, out);
    let left = u16::try_from(ed.scroll.left).unwrap_or(u16::MAX);
    Paragraph::new(lines).scroll((0, left)).render(text, out);
    if let Some(status) = status {
        Paragraph::new(status_line(ed)).render(status, out);
    }
}

fn gutter_marker(worst: Option<Severity>) -> Span<'static> {
    match worst {
        Some(Severity::Error) => Span::styled("●", Style::default().fg(t::MAGENTA)),
        Some(Severity::Warning) => Span::styled("▲", Style::default().fg(t::YELLOW)),
        None => Span::raw(" "),
    }
}

fn severity_style(severity: Severity) -> Style {
    match severity {
        Severity::Error => theme::editor_error(),
        Severity::Warning => theme::editor_warning(),
    }
}

/// Cells to underline, errors first. A diagnostic with an empty range marks
/// the char at its offset, or the cell past the end of its line.
fn marks(buf: &Buffer, diagnostics: &[Diagnostic]) -> Vec<(Range<usize>, Severity)> {
    let mut marks: Vec<_> = diagnostics
        .iter()
        .map(|d| {
            let range = if d.range.is_empty() {
                let at = d.range.start;
                let width = buf.char_after(at).filter(|&c| c != '\n').map_or(1, char::len_utf8);
                at..at + width
            } else {
                d.range.clone()
            };
            (range, d.severity)
        })
        .collect();
    marks.sort_by_key(|(_, severity)| *severity);
    marks
}

fn status_line(ed: &CodeEditor) -> Line<'static> {
    let Some(focus) = ed.diagnostic_at_caret() else {
        return Line::from(Span::styled("✓ no syntax errors", Style::default().fg(t::MUTED)));
    };
    let count = |severity| ed.diagnostics.iter().filter(|d| d.severity == severity).count();
    let mut spans = Vec::new();
    for (severity, noun, glyph, color) in [
        (Severity::Error, "error", "●", t::MAGENTA),
        (Severity::Warning, "warning", "▲", t::YELLOW),
    ] {
        let n = count(severity);
        if n > 0 {
            let plural = if n == 1 { "" } else { "s" };
            spans.push(Span::styled(format!("{glyph} {n} {noun}{plural} · "), Style::default().fg(color)));
        }
    }
    let line = ed.buffer.pos(focus.range.start).line + 1;
    spans.push(Span::styled(format!("Ln {line}: {}", focus.message), Style::default().fg(t::FG)));
    Line::from(spans)
}

/// One buffer line as spans: syntax color per char, then diagnostics, then
/// selection, then the caret.
fn line_spans(
    range: Range<usize>,
    text: &str,
    base: &[Span<'static>],
    marks: &[(Range<usize>, Severity)],
    selection: &Range<usize>,
    caret: Option<usize>,
) -> Line<'static> {
    let mut base_styles = base
        .iter()
        .flat_map(|s| s.content.chars().map(move |_| s.style));
    let marked = |off: usize| marks.iter().find(|(r, _)| r.contains(&off)).map(|&(_, s)| s);
    let style_at = |off: usize, style: Style| {
        let style = marked(off).map_or(style, |s| style.patch(severity_style(s)));
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
    if caret == Some(end) || newline_selected || marked(end).is_some() {
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
