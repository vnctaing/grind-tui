use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::theme::t;

const LABELS: [&str; 3] = ["Input:", "Output:", "Explanation:"];

/// Renders the markdown-lite grammar emitted by scripts/gen_catalog.py.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    text.lines().map(render_line).collect()
}

fn render_line(line: &str) -> Line<'static> {
    if line.trim().is_empty() {
        return Line::default();
    }
    if let Some(code) = line.strip_prefix("    ") {
        return pre_line(code);
    }
    if let Some(title) = example_title(line) {
        return Line::from(super::theme::pill(title, t::BG, t::PURPLE));
    }
    if let Some(item) = line.strip_prefix("- ") {
        let mut spans = vec![Span::styled("• ", Style::default().fg(t::MAGENTA))];
        spans.extend(inline(item));
        return Line::from(spans);
    }
    Line::from(inline(line))
}

fn example_title(line: &str) -> Option<&str> {
    line.strip_prefix("**")?
        .strip_suffix("**")
        .filter(|inner| inner.starts_with("Example "))
}

fn pre_line(code: &str) -> Line<'static> {
    let body = code.trim_start();
    let indent = &code[..code.len() - body.len()];
    let mut spans = vec![
        Span::styled("│ ", Style::default().fg(t::BORDER)),
        Span::raw(indent.to_string()),
    ];
    match LABELS.iter().find(|label| body.starts_with(**label)) {
        Some(label) => {
            let label_style = Style::default().fg(t::CYAN).add_modifier(Modifier::BOLD);
            spans.push(Span::styled(label.to_string(), label_style));
            spans.push(Span::styled(
                body[label.len()..].to_string(),
                Style::default().fg(t::FG),
            ));
        }
        None => spans.push(Span::styled(body.to_string(), Style::default().fg(t::FG))),
    }
    Line::from(spans)
}

fn inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, part) in text.split("[image]").enumerate() {
        if i > 0 {
            spans.push(Span::styled("[image]", Style::default().fg(t::MUTED)));
        }
        spans.extend(bold_and_code(part));
    }
    spans
}

fn bold_and_code(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut bold = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                push_plain(&mut spans, &mut buf, bold);
                let code: String = chars.by_ref().take_while(|&c| c != '`').collect();
                if !code.is_empty() {
                    spans.push(Span::styled(
                        code,
                        Style::default().fg(t::CYAN).bg(t::SELECTED_BG),
                    ));
                }
            }
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                push_plain(&mut spans, &mut buf, bold);
                bold = !bold;
            }
            _ => buf.push(c),
        }
    }
    push_plain(&mut spans, &mut buf, bold);
    spans
}

fn push_plain(spans: &mut Vec<Span<'static>>, buf: &mut String, bold: bool) {
    if buf.is_empty() {
        return;
    }
    let style = if bold {
        Style::default().fg(t::FG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t::FG)
    };
    spans.push(Span::styled(std::mem::take(buf), style));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_spans<'a>(lines: &'a [Line<'static>]) -> Vec<&'a Span<'static>> {
        lines.iter().flat_map(|l| l.spans.iter()).collect()
    }

    fn span<'a>(lines: &'a [Line<'static>], text: &str) -> &'a Span<'static> {
        all_spans(lines)
            .into_iter()
            .find(|s| s.content == text)
            .unwrap_or_else(|| panic!("no span {text:?} in {lines:?}"))
    }

    #[test]
    fn bold_and_inline_code_are_styled() {
        let lines = render_markdown("a **b** `c`");
        assert!(
            span(&lines, "b")
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(span(&lines, "c").style.fg, Some(t::CYAN));
        assert_eq!(span(&lines, "c").style.bg, Some(t::SELECTED_BG));
        assert_eq!(span(&lines, "a ").style.fg, Some(t::FG));
        assert!(
            !span(&lines, "a ")
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn no_markers_survive() {
        let text = "Given **n**, return `x`.\n\n**Example 1:**\n\n- one `two`\n\n    Input: nums = [1]\n    Output: 1\n\nSee [image] here";
        let lines = render_markdown(text);
        for s in all_spans(&lines) {
            assert!(
                !s.content.contains("**"),
                "leftover bold in {:?}",
                s.content
            );
            assert!(
                !s.content.contains('`'),
                "leftover backtick in {:?}",
                s.content
            );
        }
    }

    #[test]
    fn example_title_is_purple_pill() {
        let lines = render_markdown("**Example 1:**");
        let pill = span(&lines, " Example 1: ");
        assert_eq!(pill.style.bg, Some(t::PURPLE));
        assert_eq!(pill.style.fg, Some(t::BG));
    }

    #[test]
    fn pre_labels_are_cyan_bold() {
        let lines = render_markdown("    Input: nums = [1]");
        let label = span(&lines, "Input:");
        assert_eq!(label.style.fg, Some(t::CYAN));
        assert!(label.style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(span(&lines, " nums = [1]").style.fg, Some(t::FG));
    }

    #[test]
    fn bullets_are_magenta() {
        let lines = render_markdown("- item");
        assert_eq!(span(&lines, "• ").style.fg, Some(t::MAGENTA));
        assert_eq!(span(&lines, "item").style.fg, Some(t::FG));
    }

    #[test]
    fn image_placeholder_is_muted() {
        let lines = render_markdown("see [image]");
        assert_eq!(span(&lines, "[image]").style.fg, Some(t::MUTED));
    }
}
