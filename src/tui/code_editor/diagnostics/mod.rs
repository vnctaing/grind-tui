mod lint;
mod scan;

use std::ops::Range;
use tree_sitter::{Node, Parser};

pub const MAX_DIAGNOSTICS: usize = 50;

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
}

impl Checker {
    pub fn new() -> Checker {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("bundled grammar matches the tree-sitter ABI");
        Checker { parser }
    }

    pub fn check(&mut self, text: &str) -> Vec<Diagnostic> {
        let scanned = scan::scan(text);
        let mut found = scanned.diagnostics;
        let mut parse_errors = Vec::new();
        if let Some(tree) = self.parser.parse(text, None) {
            found.extend(lint::empty_if(&scanned.blank));
            collect(tree.root_node(), text, &scanned.blank, &mut parse_errors);
        }
        let mut all = merge(found, scanned.unclosed, parse_errors);
        all.sort_by(|a, b| (a.range.start, a.severity).cmp(&(b.range.start, b.severity)));
        all.dedup_by(|a, b| a.range.start == b.range.start && a.message == b.message);
        all.truncate(MAX_DIAGNOSTICS);
        all
    }
}

/// A tree-sitter error together with the full span the parser gave up on,
/// which is wider than the range shown to the user.
struct ParseError {
    span: Range<usize>,
    missing: Option<String>,
    diagnostic: Diagnostic,
}

fn collect(node: Node, text: &str, blank: &[u8], out: &mut Vec<ParseError>) {
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
            diagnostic: describe_error(node, text, blank),
        });
        return;
    }
    if !node.has_error() {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, text, blank, out);
    }
}

fn describe_error(node: Node, text: &str, blank: &[u8]) -> Diagnostic {
    let last = leaf(node, |n| n.child_count().checked_sub(1).and_then(|i| n.child(i)));
    if dangling_operator(last, blank) {
        let message = format!("Expected an expression after '{}'", last.kind());
        return Diagnostic::error(last.byte_range(), Source::Syntax, message);
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
