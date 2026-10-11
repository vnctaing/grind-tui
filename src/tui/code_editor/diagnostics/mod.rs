mod lint;
mod scan;

use crate::catalog::Lang;
use std::ops::Range;
use tree_sitter::{Node, Parser};

const MAX_DIAGNOSTICS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Syntax,
    Brackets,
    Lint,
}

/// `range` is a byte range of the checked text on char boundaries. An empty
/// range marks a spot where something is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range<usize>,
    pub severity: Severity,
    pub message: String,
    pub source: Source,
}

impl Diagnostic {
    fn error(range: Range<usize>, source: Source, message: impl Into<String>) -> Diagnostic {
        Diagnostic { range, severity: Severity::Error, message: message.into(), source }
    }

    fn warning(range: Range<usize>, source: Source, message: impl Into<String>) -> Diagnostic {
        Diagnostic { range, severity: Severity::Warning, message: message.into(), source }
    }
}

pub struct Checker {
    parser: Parser,
    lang: Lang,
}

impl Checker {
    pub fn new(lang: Lang) -> Checker {
        let mut parser = Parser::new();
        parser
            .set_language(&grammar(lang))
            .expect("bundled grammars match the tree-sitter ABI");
        Checker { parser, lang }
    }

    /// Syntax errors, bracket and string balance, and a few precise lints,
    /// sorted by position.
    pub fn check(&mut self, text: &str) -> Vec<Diagnostic> {
        let scanned = scan::scan(text, self.lang);
        let mut found = scanned.diagnostics;
        let mut parse_errors = Vec::new();
        if let Some(tree) = self.parser.parse(text, None) {
            found.extend(lint::lint(text, &scanned.blank, tree.root_node(), self.lang));
            collect(tree.root_node(), text, &scanned.blank, self.lang, &mut parse_errors);
        }
        let mut all = merge(found, scanned.unclosed, parse_errors);
        all.sort_by(|a, b| (a.range.start, a.severity).cmp(&(b.range.start, b.severity)));
        all.dedup_by(|a, b| a.range.start == b.range.start && a.message == b.message);
        all.truncate(MAX_DIAGNOSTICS);
        all
    }
}

fn grammar(lang: Lang) -> tree_sitter::Language {
    match lang {
        Lang::Python3 => tree_sitter_python::LANGUAGE.into(),
        Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
        Lang::Javascript => tree_sitter_javascript::LANGUAGE.into(),
        Lang::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Lang::Java => tree_sitter_java::LANGUAGE.into(),
        Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Lang::Go => tree_sitter_go::LANGUAGE.into(),
        Lang::C => tree_sitter_c::LANGUAGE.into(),
    }
}

/// A tree-sitter error together with the full span the parser gave up on,
/// which is wider than the range shown to the user.
struct ParseError {
    span: Range<usize>,
    missing: Option<String>,
    diagnostic: Diagnostic,
}

fn collect(node: Node, text: &str, blank: &[u8], lang: Lang, out: &mut Vec<ParseError>) {
    if out.len() >= MAX_DIAGNOSTICS {
        return;
    }
    if node.is_missing() {
        let at = node.start_byte();
        let kind = node.kind();
        let message = if kind.chars().any(|c| c.is_alphanumeric()) {
            format!("Missing {}", kind.replace('_', " "))
        } else {
            format!("Missing '{kind}'")
        };
        out.push(ParseError {
            span: at..at,
            missing: Some(kind.to_string()),
            diagnostic: Diagnostic::error(at..at, Source::Syntax, message),
        });
        return;
    }
    if node.is_error() {
        out.push(ParseError {
            span: node.byte_range(),
            missing: None,
            diagnostic: describe_error(node, text, blank, lang),
        });
        return;
    }
    if !node.has_error() {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, text, blank, lang, out);
    }
}

/// An ERROR span can cover a whole function, so the diagnostic narrows it
/// to one token: an operator left dangling at its end, else the spot where
/// a statement on a new line follows one with no `;`, else its first token.
fn describe_error(node: Node, text: &str, blank: &[u8], lang: Lang) -> Diagnostic {
    let last = leaf(node, |n| n.child_count().checked_sub(1).and_then(|i| n.child(i)));
    if dangling_operator(last, blank) {
        let message = format!("Expected an expression after '{}'", last.kind());
        return Diagnostic::error(last.byte_range(), Source::Syntax, message);
    }
    if matches!(lang, Lang::Rust | Lang::Java | Lang::C | Lang::Cpp) {
        let mut cursor = node.walk();
        let starts = std::iter::once(node.start_byte()).chain(node.children(&mut cursor).map(|c| c.start_byte()));
        if let Some(at) = starts.filter_map(|start| missing_semicolon(blank, start)).next() {
            return Diagnostic::error(at..at, Source::Syntax, "Missing ';'");
        }
    }
    let first = leaf(node, |n| n.child(0)).byte_range();
    let first = first.start..text[first.clone()].find('\n').map_or(first.end, |i| first.start + i);
    let message = match &text[first.clone()] {
        "" => "Syntax error".to_string(),
        shown => format!("Unexpected token '{}'", truncate(shown, 24)),
    };
    Diagnostic::error(first, Source::Syntax, message)
}

fn leaf<'t>(mut node: Node<'t>, next: impl Fn(&Node<'t>) -> Option<Node<'t>>) -> Node<'t> {
    while let Some(child) = next(&node) {
        node = child;
    }
    node
}

/// An operator token followed by nothing that could be its operand.
fn dangling_operator(token: Node, blank: &[u8]) -> bool {
    let kind = token.kind();
    let operator = !token.is_named()
        && !kind.is_empty()
        && kind.bytes().all(|c| c.is_ascii_punctuation())
        && !matches!(kind, ")" | "]" | "}" | ";" | "," | ":");
    let next = blank[token.end_byte()..].iter().find(|c| !matches!(c, b' ' | b'\t' | b'\r'));
    operator && next.is_none_or(|c| matches!(c, b'\n' | b';' | b')' | b']' | b'}'))
}

/// When the token at `start` opens its line and the code before it ends an
/// expression, the `;` between them is missing. Returns where it belongs.
fn missing_semicolon(blank: &[u8], start: usize) -> Option<usize> {
    let line_start = blank[..start].iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1);
    let opens_line = blank[line_start..start].iter().all(u8::is_ascii_whitespace);
    let starts_statement = blank.get(start).is_some_and(|&c| c.is_ascii_alphabetic() || c == b'_');
    let prev = blank[..start].iter().rposition(|c| !c.is_ascii_whitespace())?;
    let ends_expression = blank[prev].is_ascii_alphanumeric() || matches!(blank[prev], b'_' | b')' | b']' | b'x');
    (opens_line && starts_statement && ends_expression).then_some(prev + 1)
}

fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_string(),
    }
}

/// Bracket, string and lint findings are precise, so a parser ERROR whose
/// span contains one is a cascade of it and is dropped. A parser MISSING
/// closer replaces the scanner's "unclosed" for the same bracket, since it
/// marks where the closer belongs.
fn merge(mut found: Vec<Diagnostic>, unclosed: Vec<scan::Unclosed>, parse: Vec<ParseError>) -> Vec<Diagnostic> {
    found.extend(
        unclosed
            .into_iter()
            .filter(|u| {
                !parse.iter().any(|p| {
                    p.missing.as_deref() == Some(u.closer()) && p.span.start > u.at
                })
            })
            .map(scan::Unclosed::diagnostic),
    );
    let precise: Vec<usize> = found.iter().map(|d| d.range.start).collect();
    found.extend(
        parse
            .into_iter()
            .filter(|p| p.missing.is_some() || !precise.iter().any(|&at| p.span.contains(&at) || p.span.start == at))
            .map(|p| p.diagnostic),
    );
    found
}

#[cfg(test)]
mod tests;
