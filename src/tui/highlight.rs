use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::theme::t;

const JS_KEYWORDS: &[&str] = &[
    "function",
    "const",
    "let",
    "var",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "class",
    "new",
    "this",
    "import",
    "export",
    "from",
    "async",
    "await",
    "typeof",
    "instanceof",
    "null",
    "undefined",
    "true",
    "false",
    "break",
    "continue",
    "switch",
    "case",
    "default",
    "throw",
    "try",
    "catch",
];

const COMMENT: &str = "//";
pub const QUOTES: &[char] = &['"', '\'', '`'];

pub fn highlight_code(code: &str) -> Vec<Line<'static>> {
    code.lines().map(highlight_line).collect()
}

fn highlight_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if rest.starts_with(COMMENT) {
            push_span(&mut spans, rest, comment_style());
            break;
        }
        let len = if QUOTES.contains(&c) {
            string_len(rest, c)
        } else if c.is_ascii_digit() {
            take_while_len(rest, |ch| ch.is_alphanumeric() || ch == '.' || ch == '_')
        } else if c.is_alphabetic() || c == '_' {
            let len = take_while_len(rest, |ch| ch.is_alphanumeric() || ch == '_');
            let word = &rest[..len];
            if JS_KEYWORDS.contains(&word) {
                push_span(&mut spans, word, keyword_style());
                rest = &rest[len..];
                continue;
            }
            len
        } else {
            c.len_utf8()
        };
        let token = &rest[..len];
        let style = match c {
            _ if QUOTES.contains(&c) => Style::default().fg(t::MINT),
            _ if c.is_ascii_digit() => Style::default().fg(t::YELLOW),
            _ => Style::default().fg(t::FG),
        };
        push_span(&mut spans, token, style);
        rest = &rest[len..];
    }
    Line::from(spans)
}

fn string_len(s: &str, quote: char) -> usize {
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            return i + c.len_utf8();
        }
    }
    s.len()
}

fn take_while_len(s: &str, keep: impl Fn(char) -> bool) -> usize {
    s.char_indices()
        .find(|&(_, c)| !keep(c))
        .map_or(s.len(), |(i, _)| i)
}

fn push_span(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    match spans.last_mut() {
        Some(last) if last.style == style => last.content.to_mut().push_str(text),
        _ => spans.push(Span::styled(text.to_string(), style)),
    }
}

fn keyword_style() -> Style {
    Style::default().fg(t::PURPLE).add_modifier(Modifier::BOLD)
}

fn comment_style() -> Style {
    Style::default().fg(t::MUTED).add_modifier(Modifier::ITALIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_style(lines: &[Line<'static>], text: &str) -> Option<Style> {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content == text)
            .map(|s| s.style)
    }

    #[test]
    fn strings_are_mint_and_comments_muted() {
        let lines = highlight_code("x = `hi` // c");
        assert_eq!(
            span_style(&lines, "`hi`").and_then(|s| s.fg),
            Some(t::MINT)
        );
        assert_eq!(span_style(&lines, "// c").and_then(|s| s.fg), Some(t::MUTED));
        assert!(
            span_style(&lines, "// c")
                .unwrap()
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn slashes_inside_string_are_not_a_comment() {
        let lines = highlight_code("let u = \"//x\"; // note");
        assert_eq!(
            span_style(&lines, "\"//x\"").and_then(|s| s.fg),
            Some(t::MINT)
        );
        assert_eq!(
            span_style(&lines, "// note").and_then(|s| s.fg),
            Some(t::MUTED)
        );
    }

    #[test]
    fn keywords_and_numbers_are_styled() {
        let lines = highlight_code("function f() { return 42; }");
        let kw = span_style(&lines, "function").unwrap();
        assert_eq!(kw.fg, Some(t::PURPLE));
        assert!(kw.add_modifier.contains(Modifier::BOLD));
        assert_eq!(span_style(&lines, "42").and_then(|s| s.fg), Some(t::YELLOW));
    }

    #[test]
    fn plain_identifiers_keep_foreground() {
        let lines = highlight_code("total");
        assert_eq!(span_style(&lines, "total").and_then(|s| s.fg), Some(t::FG));
    }
}
