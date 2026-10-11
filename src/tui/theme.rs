use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
};

use crate::catalog::Difficulty;

/// Display name in the UI chrome. The binary and crate keep their own names.
pub const BRAND: &str = "LEETNET";

pub mod t {
    use ratatui::style::Color;

    pub const BG: Color = Color::Rgb(10, 8, 20);
    pub const FG: Color = Color::Rgb(224, 222, 244);
    pub const MUTED: Color = Color::Rgb(122, 116, 160);
    pub const BORDER: Color = Color::Rgb(58, 46, 102);
    pub const SELECTED_BG: Color = Color::Rgb(34, 20, 74);
    pub const MAGENTA: Color = Color::Rgb(255, 42, 109);
    pub const CYAN: Color = Color::Rgb(5, 217, 232);
    pub const YELLOW: Color = Color::Rgb(249, 240, 2);
    pub const PURPLE: Color = Color::Rgb(185, 103, 255);
    pub const MINT: Color = Color::Rgb(5, 255, 161);
}

pub fn difficulty_color(d: Difficulty) -> Color {
    match d {
        Difficulty::Easy => t::MINT,
        Difficulty::Medium => t::YELLOW,
        Difficulty::Hard => t::MAGENTA,
    }
}

pub fn difficulty_label(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Easy => "Easy",
        Difficulty::Medium => "Medium",
        Difficulty::Hard => "Hard",
    }
}

pub fn pill(text: &str, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

pub fn gradient(text: &str, from: Color, to: Color) -> Vec<Span<'static>> {
    let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (from, to) else {
        return vec![Span::styled(
            text.to_string(),
            Style::default().fg(from).add_modifier(Modifier::BOLD),
        )];
    };
    let steps = text.chars().count().max(2) as f32 - 1.0;
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let k = i as f32 / steps;
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * k).round() as u8;
            let color = Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2));
            Span::styled(
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

/// `key desc • key desc`, in the style of Bubble Tea's help bubble.
pub fn help_line(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" • ", Style::default().fg(t::BORDER)));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(t::CYAN).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {desc}"),
            Style::default().fg(t::MUTED),
        ));
    }
    Line::from(spans)
}

pub fn panel(title: &str, hint: &str, focused: bool) -> Block<'static> {
    let (border, title_span) = if focused {
        (t::MAGENTA, pill(title, t::BG, t::MAGENTA))
    } else {
        (
            t::BORDER,
            Span::styled(
                format!(" {title} "),
                Style::default().fg(t::MUTED).add_modifier(Modifier::BOLD),
            ),
        )
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(t::BG))
        .title(Line::from(title_span));
    if !hint.is_empty() {
        let hint_span = Span::styled(format!(" {hint} "), Style::default().fg(t::MUTED));
        block = block.title_top(Line::from(hint_span).right_aligned());
    }
    block
}

pub fn editor_cursor() -> Style {
    Style::default().fg(t::BG).bg(t::CYAN)
}

pub fn editor_selection() -> Style {
    Style::default().bg(t::BORDER)
}

pub fn progress_bar(done: usize, total: usize, width: u16) -> Vec<Span<'static>> {
    let width = usize::from(width);
    let filled = if total == 0 {
        0
    } else {
        done.min(total) * width / total
    };
    vec![
        Span::styled("━".repeat(filled), Style::default().fg(t::MAGENTA)),
        Span::styled("─".repeat(width - filled), Style::default().fg(t::BORDER)),
        Span::styled(format!(" {done}/{total}"), Style::default().fg(t::MUTED)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled_glyphs(spans: &[Span]) -> usize {
        spans.iter().map(|s| s.content.matches('━').count()).sum()
    }

    #[test]
    fn progress_bar_fills_by_ratio() {
        let spans = progress_bar(3, 10, 10);
        assert_eq!(filled_glyphs(&spans), 3);
        assert_eq!(spans[0].style.fg, Some(t::MAGENTA));
        assert_eq!(spans[1].content.matches('─').count(), 7);
    }

    #[test]
    fn progress_bar_handles_empty_total() {
        assert_eq!(filled_glyphs(&progress_bar(0, 0, 10)), 0);
    }

    #[test]
    fn difficulty_colors_match_palette() {
        assert_eq!(difficulty_color(Difficulty::Easy), t::MINT);
        assert_eq!(difficulty_color(Difficulty::Medium), t::YELLOW);
        assert_eq!(difficulty_color(Difficulty::Hard), t::MAGENTA);
    }
}
