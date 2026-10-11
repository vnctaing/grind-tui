use super::{Diagnostic, Source};
use crate::catalog::Lang;
use std::ops::Range;
use tree_sitter::Node;

const ASSIGN_IN_CONDITION: &str = "Assignment in condition: use '==' to compare";

/// Mistakes a beginner makes often that the grammar alone misreports or
/// accepts. `blank` comes from the scanner.
pub fn lint(text: &str, blank: &[u8], root: Node, lang: Lang) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    match lang {
        Lang::Python3 => {
            print_statements(root, &mut out);
            for line in logical_lines(blank) {
                python_line(text.as_bytes(), blank, line, &mut out);
            }
        }
        Lang::Rust => rust_conditions(blank, &mut out),
        Lang::Java | Lang::Cpp | Lang::C | Lang::Javascript | Lang::Typescript => empty_if(blank, &mut out),
        Lang::Go => {}
    }
    out
}

fn print_statements(node: Node, out: &mut Vec<Diagnostic>) {
    if node.kind() == "print_statement" && !node.has_error() {
        let at = node.start_byte();
        out.push(Diagnostic::error(at..at + "print".len(), Source::Lint, "print is a function in Python 3: print(...)"));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_statements(child, out);
    }
}

/// Python statements: physical lines joined while a bracket is open or the
/// line ends in a backslash.
fn logical_lines(blank: &[u8]) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (i, &c) in blank.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'\n' if depth == 0 && blank[start..i].last() != Some(&b'\\') => {
                lines.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
    }
    lines.push(start..blank.len());
    lines
}

const PY_HEADERS: &[&[u8]] = &[
    b"if", b"elif", b"else", b"for", b"while", b"def", b"class", b"try", b"except", b"finally", b"with",
];

fn python_line(text: &[u8], blank: &[u8], line: Range<usize>, out: &mut Vec<Diagnostic>) {
    let code = &blank[line.clone()];
    let indent = code.iter().take_while(|c| matches!(c, b' ' | b'\t')).count();
    let lead = &text[line.start..line.start + indent];
    if lead.contains(&b' ') && lead.contains(&b'\t') && indent < code.len() {
        out.push(Diagnostic::warning(line.start..line.start + indent, Source::Lint, "Indentation mixes tabs and spaces"));
    }
    let mut word_at = indent;
    let mut word = leading_word(&code[word_at..]);
    if word == b"async" {
        word_at += word.len();
        word_at += code[word_at..].iter().take_while(|c| matches!(c, b' ' | b'\t')).count();
        word = leading_word(&code[word_at..]);
    }
    if !PY_HEADERS.contains(&word) {
        return;
    }
    let after = word_at + word.len();
    if code.get(after).is_some_and(|&c| is_ident_byte(c)) {
        return;
    }
    let colon = depth_zero(code, after).find(|&i| code[i] == b':' && code.get(i + 1) != Some(&b'='));
    if matches!(word, b"if" | b"elif" | b"while") {
        let cond_end = colon.unwrap_or(code.len());
        if let Some(i) = depth_zero(code, after).take_while(|&i| i < cond_end).find(|&i| is_assignment(code, i, b"=!<>:+-*/%&|^@~")) {
            out.push(Diagnostic::error(line.start + i..line.start + i + 1, Source::Lint, ASSIGN_IN_CONDITION));
        }
    }
    if colon.is_none() && balanced(code) {
        let end = code.iter().rposition(|c| !c.is_ascii_whitespace() && *c != b'\\').map_or(0, |p| p + 1);
        let at = line.start + end;
        out.push(Diagnostic::error(at..at, Source::Lint, "Missing ':'"));
    }
}

/// `if`/`while` conditions with a bare `=`, which Rust rejects as a type error.
fn rust_conditions(blank: &[u8], out: &mut Vec<Diagnostic>) {
    for kw in [&b"if"[..], b"while"] {
        for at in words(blank, kw) {
            let start = at + kw.len();
            let Some(body) = depth_zero(blank, start).find(|&i| matches!(blank[i], b'{' | b';')) else { continue };
            if blank[body] != b'{' || words(&blank[start..body], b"let").next().is_some() {
                continue;
            }
            if let Some(i) = depth_zero(blank, start).take_while(|&i| i < body).find(|&i| is_assignment(blank, i, b"=!<>+-*/%&|^")) {
                out.push(Diagnostic::error(i..i + 1, Source::Lint, ASSIGN_IN_CONDITION));
            }
        }
    }
}

/// `if (cond);` runs nothing when `cond` holds.
fn empty_if(blank: &[u8], out: &mut Vec<Diagnostic>) {
    for at in words(blank, b"if") {
        let open = skip_space(blank, at + 2);
        if blank.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = closing_paren(blank, open) else { continue };
        let semi = skip_space(blank, close + 1);
        if blank.get(semi) == Some(&b';') {
            out.push(Diagnostic::warning(semi..semi + 1, Source::Lint, "Empty statement after 'if'"));
        }
    }
}

/// A lone `=` at `i`: not `==`, `=>`, or the end of a compound operator.
fn is_assignment(code: &[u8], i: usize, compound: &[u8]) -> bool {
    code[i] == b'='
        && !matches!(code.get(i + 1), Some(b'=' | b'>'))
        && !(i > 0 && compound.contains(&code[i - 1]))
}

/// Offsets from `from` that sit outside any bracket opened after `from`,
/// stopping at a closer that would go below `from`'s depth.
fn depth_zero(code: &[u8], from: usize) -> impl Iterator<Item = usize> + '_ {
    let mut depth = 0usize;
    (from..code.len()).map_while(move |i| match code[i] {
        b'(' | b'[' | b'{' => {
            depth += 1;
            Some((depth == 1).then_some(i))
        }
        b')' | b']' | b'}' => {
            if depth == 0 {
                return None;
            }
            depth -= 1;
            Some(None)
        }
        _ => Some((depth == 0).then_some(i)),
    })
    .flatten()
}

fn closing_paren(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &c) in code.iter().enumerate().skip(open) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (c == b')').then_some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn balanced(code: &[u8]) -> bool {
    let mut depth = 0isize;
    for &c in code {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

fn words<'a>(code: &'a [u8], word: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    code.windows(word.len()).enumerate().filter_map(move |(i, w)| {
        let before = i == 0 || !is_ident_byte(code[i - 1]);
        let after = code.get(i + word.len()).is_none_or(|&c| !is_ident_byte(c));
        (w == word && before && after).then_some(i)
    })
}

fn leading_word(code: &[u8]) -> &[u8] {
    let len = code.iter().take_while(|&&c| is_ident_byte(c)).count();
    &code[..len]
}

fn skip_space(code: &[u8], from: usize) -> usize {
    from + code[from.min(code.len())..].iter().take_while(|c| matches!(c, b' ' | b'\t')).count()
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}
