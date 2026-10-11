use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ── Syntax highlighting ───────────────────────────────────────────────────────

fn keywords_for_lang(lang: &str) -> &'static [&'static str] {
    match lang {
        "python3" | "python" => &["def","class","return","if","elif","else","for","while","in","and","or","not","import","from","pass","break","continue","lambda","with","as","try","except","finally","raise","True","False","None","self"],
        "rust" => &["fn","let","mut","pub","use","struct","enum","impl","trait","return","if","else","for","while","match","mod","crate","super","self","Self","type","where","async","await","move","ref","in","loop","break","continue","const","static"],
        "javascript" | "typescript" => &["function","const","let","var","return","if","else","for","while","class","import","export","from","async","await","new","this","typeof","instanceof","null","undefined","true","false"],
        _ => &[],
    }
}

pub(super) fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let keywords = keywords_for_lang(lang);
    code.lines().map(|line| {
        let comment_start = match lang {
            "python3" | "python" => line.find('#'),
            "rust" | "javascript" | "typescript" | "cpp" | "java" | "go" => line.find("//"),
            _ => None,
        };
        if let Some(pos) = comment_start {
            let (code_part, comment_part) = line.split_at(pos);
            let mut spans = tokenize_spans(code_part, keywords);
            spans.push(Span::styled(comment_part.to_string(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
            Line::from(spans)
        } else {
            Line::from(tokenize_spans(line, keywords))
        }
    }).collect()
}

fn tokenize_spans(line: &str, keywords: &[&str]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if !current.is_empty() {
                spans.push(word_span(current.clone(), keywords));
                current.clear();
            }
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if !current.is_empty() { spans.push(word_span(current, keywords)); }
    spans
}

fn word_span(word: String, keywords: &[&str]) -> Span<'static> {
    if keywords.contains(&word.as_str()) {
        Span::styled(word, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
    } else if word.chars().next().map(|c| c.is_numeric()).unwrap_or(false) {
        Span::styled(word, Style::default().fg(Color::Cyan))
    } else {
        Span::raw(word)
    }
}
