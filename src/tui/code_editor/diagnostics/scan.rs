use super::{Diagnostic, Source};
use crate::catalog::Lang;
use std::ops::Range;

/// `blank` is the text with comment and preprocessor bytes replaced by
/// spaces, keeping their newlines, and string bytes, newlines included, by
/// `x`, so lints can read code by byte without lexing again. A string still
/// reads as one token on one line. Offsets match the text. `disabled` holds
/// the C and C++ regions under `#if 0`, which the compiler never reads.
pub struct Scan {
    pub blank: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
    pub unclosed: Vec<Unclosed>,
    pub disabled: Vec<Range<usize>>,
}

/// An opening bracket with no closer before the end of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unclosed {
    pub at: usize,
    pub open: u8,
}

impl Unclosed {
    pub fn closer(self) -> &'static str {
        match self.open {
            b'(' => ")",
            b'[' => "]",
            _ => "}",
        }
    }

    pub fn diagnostic(self) -> Diagnostic {
        Diagnostic::error(self.at..self.at + 1, Source::Brackets, format!("Unclosed '{}'", self.open as char))
    }
}

enum Frame {
    Bracket { open: u8, at: usize },
    /// `${` at `at` inside a JS template string that opened at `template`.
    Interp { template: usize, at: usize },
}

enum Literal {
    /// Ends at `close`, honors backslash escapes, and stops at a newline
    /// unless the string type allows them.
    Escaped { close: &'static [u8], multiline: bool },
    Raw { close: Vec<u8> },
}

pub fn scan(text: &str, lang: Lang) -> Scan {
    let mut s = Scanner {
        bytes: text.as_bytes(),
        blank: text.as_bytes().to_vec(),
        lang,
        stack: Vec::new(),
        diagnostics: Vec::new(),
        unclosed: Vec::new(),
        disabled: Vec::new(),
        swallowed: false,
        regex_allowed: true,
    };
    s.run(0);
    let open = if s.swallowed { Vec::new() } else { std::mem::take(&mut s.stack) };
    for frame in open {
        match frame {
            Frame::Bracket { open, at } => s.unclosed.push(Unclosed { at, open }),
            Frame::Interp { template, .. } => s.diagnostics.push(unterminated(template, s.bytes, "template string")),
        }
    }
    Scan { blank: s.blank, diagnostics: s.diagnostics, unclosed: s.unclosed, disabled: s.disabled }
}

struct Scanner<'a> {
    bytes: &'a [u8],
    blank: Vec<u8>,
    lang: Lang,
    stack: Vec<Frame>,
    diagnostics: Vec<Diagnostic>,
    unclosed: Vec<Unclosed>,
    disabled: Vec<Range<usize>>,
    /// A literal or comment ran to the end of the text, so the brackets still
    /// open were most likely swallowed by it rather than left open.
    swallowed: bool,
    /// Whether a `/` here would start a JS regex literal rather than divide.
    regex_allowed: bool,
}

impl Scanner<'_> {
    fn run(&mut self, mut i: usize) {
        let b = self.bytes;
        let line_comment: &[u8] = if self.lang == Lang::Python3 { b"#" } else { b"//" };
        while i < b.len() {
            let c = b[i];
            let rest = &b[i..];
            let was_regex_allowed = self.regex_allowed;
            self.regex_allowed = false;
            if c == b'#' && matches!(self.lang, Lang::C | Lang::Cpp) && self.at_line_start(i) {
                i = self.erase_directive(i);
                self.regex_allowed = was_regex_allowed;
            } else if rest.starts_with(line_comment) {
                let end = line_end(b, i);
                self.erase_comment(i, end);
                i = end;
                self.regex_allowed = was_regex_allowed;
            } else if self.lang != Lang::Python3 && rest.starts_with(b"/*") {
                i = self.block_comment(i);
                self.regex_allowed = was_regex_allowed;
            } else if let Some((open_len, literal)) = self.literal_at(i) {
                i = self.literal(i, open_len, literal);
            } else if c == b'\'' && self.lang == Lang::Rust {
                i = self.rust_quote(i);
            } else if c == b'`' && matches!(self.lang, Lang::Javascript | Lang::Typescript) {
                i = self.template(i, i + 1);
            } else if c == b'/' && was_regex_allowed && matches!(self.lang, Lang::Javascript | Lang::Typescript) {
                i = self.regex(i);
            } else if is_ident_byte(c) && !c.is_ascii_digit() {
                let end = ident_end(b, i);
                self.regex_allowed = REGEX_KEYWORDS.contains(&&b[i..end]);
                i = end;
            } else if c.is_ascii_digit() {
                i = number_end(b, i, self.lang);
            } else if matches!(c, b'(' | b'[' | b'{') {
                self.stack.push(Frame::Bracket { open: c, at: i });
                self.regex_allowed = true;
                i += 1;
            } else if matches!(c, b')' | b']' | b'}') {
                if c == b'}' && matches!(self.stack.last(), Some(Frame::Interp { .. })) {
                    let Some(Frame::Interp { template, .. }) = self.stack.pop() else { unreachable!() };
                    i = self.template(template, i + 1);
                } else {
                    let opener = self.close(c, i);
                    self.regex_allowed = match c {
                        b'}' => true,
                        b')' => opener.is_some_and(|at| self.opens_statement_header(at)),
                        _ => false,
                    };
                    i += 1;
                }
            } else {
                self.regex_allowed = !c.is_ascii_whitespace() || was_regex_allowed;
                i += 1;
            }
        }
    }

    /// The literal opening at `i`, as the length of its opening delimiter.
    fn literal_at(&self, i: usize) -> Option<(usize, Literal)> {
        let rest = &self.bytes[i..];
        let escaped = |close: &'static [u8], multiline| Some((close.len(), Literal::Escaped { close, multiline }));
        let raw = match self.lang {
            Lang::Rust if self.at_word_start(i) => rust_raw(rest),
            Lang::Cpp if self.at_word_start(i) => cpp_raw(rest),
            _ => None,
        };
        if raw.is_some() {
            return raw;
        }
        match self.lang {
            Lang::Python3 if rest.starts_with(b"\"\"\"") => escaped(b"\"\"\"", true),
            Lang::Python3 if rest.starts_with(b"'''") => escaped(b"'''", true),
            Lang::Java if rest.starts_with(b"\"\"\"") => escaped(b"\"\"\"", true),
            Lang::Rust if rest[0] == b'"' => escaped(b"\"", true),
            Lang::Go if rest[0] == b'`' => Some((1, Literal::Raw { close: b"`".to_vec() })),
            _ => match rest[0] {
                b'"' => escaped(b"\"", false),
                b'\'' if self.lang != Lang::Rust => escaped(b"'", false),
                _ => None,
            },
        }
    }

    fn at_word_start(&self, i: usize) -> bool {
        i == 0 || !is_ident_byte(self.bytes[i - 1])
    }

    fn at_line_start(&self, i: usize) -> bool {
        self.bytes[..i].iter().rev().take_while(|&&c| c != b'\n').all(|c| matches!(c, b' ' | b'\t'))
    }

    fn opens_statement_header(&self, open: usize) -> bool {
        let end = self.blank[..open].iter().rposition(|c| !c.is_ascii_whitespace()).map_or(0, |p| p + 1);
        let start = self.blank[..end].iter().rposition(|&c| !is_ident_byte(c)).map_or(0, |p| p + 1);
        matches!(&self.blank[start..end], b"if" | b"while" | b"for" | b"with")
    }

    fn erase_directive(&mut self, start: usize) -> usize {
        let end = directive_end(self.bytes, start);
        self.erase_comment(start, end);
        if !matches!(directive_words(&self.bytes[start..end]), (b"if", b"0")) {
            return end;
        }
        let mut depth = 0usize;
        let mut line = end;
        while line < self.bytes.len() {
            let next = directive_end(self.bytes, line + 1);
            let text = &self.bytes[line + 1..next];
            let name = if text.trim_ascii_start().starts_with(b"#") { directive_words(text).0 } else { b"" };
            match name {
                b"if" | b"ifdef" | b"ifndef" => depth += 1,
                b"endif" if depth > 0 => depth -= 1,
                b"else" | b"elif" | b"endif" if depth == 0 => {
                    self.erase_comment(end, line + 1);
                    self.disabled.push(start..line + 1);
                    return line + 1;
                }
                _ => {}
            }
            line = next;
        }
        self.erase_comment(end, self.bytes.len());
        self.disabled.push(start..self.bytes.len());
        self.bytes.len()
    }

    /// Returns where code resumes.
    fn literal(&mut self, start: usize, open_len: usize, literal: Literal) -> usize {
        let b = self.bytes;
        let mut j = start + open_len;
        let end = match &literal {
            Literal::Escaped { close, multiline } => loop {
                if j >= b.len() {
                    break None;
                }
                if b[j] == b'\\' {
                    j = (j + 2).min(b.len());
                } else if b[j..].starts_with(close) {
                    break Some(j + close.len());
                } else if b[j] == b'\n' && !multiline {
                    break None;
                } else {
                    j += 1;
                }
            },
            Literal::Raw { close } => find(b, start + open_len, close).map(|at| at + close.len()),
        };
        match end {
            Some(end) => {
                self.erase(start, end);
                end
            }
            None => {
                self.diagnostics.push(unterminated(start, b, "string"));
                let stop = match literal {
                    Literal::Escaped { multiline: false, .. } => j,
                    _ => b.len(),
                };
                self.swallowed |= stop == b.len();
                self.erase(start, stop);
                stop
            }
        }
    }

    /// A Rust `'` is a char literal (`'a'`, `'\n'`) or a lifetime (`'a`).
    fn rust_quote(&mut self, i: usize) -> usize {
        let b = self.bytes;
        if b.get(i + 1) == Some(&b'\\') {
            return self.literal(i, 1, Literal::Escaped { close: b"'", multiline: false });
        }
        let len = b.get(i + 1).map_or(0, |&c| utf8_len(c));
        if len > 0 && b.get(i + 1 + len) == Some(&b'\'') && b[i + 1] != b'\n' {
            self.erase(i, i + len + 2);
            return i + len + 2;
        }
        i + 1
    }

    /// Scans template text from `from` up to its closing backtick, or up to
    /// a `${`, which hands back to code mode. The byte before `from` is the
    /// opening backtick or the `}` that closed an interpolation.
    fn template(&mut self, start: usize, from: usize) -> usize {
        let b = self.bytes;
        let mut j = from;
        while j < b.len() {
            if b[j] == b'\\' {
                j = (j + 2).min(b.len());
            } else if b[j] == b'`' {
                self.erase(from - 1, j + 1);
                return j + 1;
            } else if b[j..].starts_with(b"${") {
                self.erase(from - 1, j + 2);
                self.stack.push(Frame::Interp { template: start, at: j });
                self.regex_allowed = true;
                return j + 2;
            } else {
                j += 1;
            }
        }
        let interp = self.stack.iter().rev().find_map(|f| match f {
            Frame::Interp { at, .. } => Some(*at),
            Frame::Bracket { .. } => None,
        });
        self.diagnostics.push(match interp {
            Some(at) => Diagnostic::error(at..at + 2, Source::Brackets, "Unclosed '${'"),
            None => unterminated(start, b, "template string"),
        });
        self.swallowed = true;
        self.erase(from - 1, b.len());
        b.len()
    }

    fn regex(&mut self, start: usize) -> usize {
        let b = self.bytes;
        let mut j = start + 1;
        let mut class = false;
        while j < b.len() && b[j] != b'\n' {
            match b[j] {
                b'\\' => j += 1,
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => {
                    let end = ident_end(b, j + 1);
                    self.erase(start, end);
                    return end;
                }
                _ => {}
            }
            j += 1;
        }
        start + 1
    }

    fn block_comment(&mut self, start: usize) -> usize {
        let b = self.bytes;
        let nested = self.lang == Lang::Rust;
        let mut depth = 0;
        let mut j = start;
        while j + 1 < b.len() {
            if b[j..].starts_with(b"/*") && (nested || depth == 0) {
                depth += 1;
                j += 2;
            } else if b[j..].starts_with(b"*/") {
                depth -= 1;
                j += 2;
                if depth == 0 {
                    self.erase_comment(start, j);
                    return j;
                }
            } else {
                j += 1;
            }
        }
        self.diagnostics.push(Diagnostic::error(start..start + 2, Source::Brackets, "Unterminated comment"));
        self.swallowed = true;
        self.erase_comment(start, b.len());
        b.len()
    }

    fn close(&mut self, c: u8, at: usize) -> Option<usize> {
        let open = opener_of(c);
        let reachable = self
            .stack
            .iter()
            .rev()
            .take_while(|f| matches!(f, Frame::Bracket { .. }))
            .position(|f| matches!(f, Frame::Bracket { open: o, .. } if *o == open));
        match reachable {
            Some(depth) => {
                for _ in 0..depth {
                    if let Some(Frame::Bracket { open, at }) = self.stack.pop() {
                        self.unclosed.push(Unclosed { at, open });
                    }
                }
                let Some(Frame::Bracket { at, .. }) = self.stack.pop() else { unreachable!() };
                Some(at)
            }
            None => {
                let message = format!("Unexpected '{}'", c as char);
                self.diagnostics.push(Diagnostic::error(at..at + 1, Source::Brackets, message));
                None
            }
        }
    }

    fn erase(&mut self, from: usize, to: usize) {
        self.blank[from..to].fill(b'x');
    }

    fn erase_comment(&mut self, from: usize, to: usize) {
        for byte in &mut self.blank[from..to] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
}

const REGEX_KEYWORDS: &[&[u8]] = &[
    b"return", b"typeof", b"case", b"do", b"else", b"in", b"of", b"new", b"delete", b"void", b"throw",
    b"yield", b"await",
];

fn unterminated(start: usize, b: &[u8], what: &str) -> Diagnostic {
    Diagnostic::error(start..line_end(b, start), Source::Brackets, format!("Unterminated {what}"))
}

/// `r"..."`, `r#"..."#`, `br"..."`, `cr"..."`.
fn rust_raw(rest: &[u8]) -> Option<(usize, Literal)> {
    let prefix = if rest.starts_with(b"br") || rest.starts_with(b"cr") { 2 } else if rest[0] == b'r' { 1 } else { 0 };
    if prefix == 0 {
        return None;
    }
    let hashes = rest[prefix..].iter().take_while(|&&c| c == b'#').count();
    if rest.get(prefix + hashes) != Some(&b'"') {
        return None;
    }
    let mut close = vec![b'"'];
    close.extend(std::iter::repeat_n(b'#', hashes));
    Some((prefix + hashes + 1, Literal::Raw { close }))
}

/// `R"delim(...)delim"` with an optional `u8`, `u`, `U` or `L` prefix.
fn cpp_raw(rest: &[u8]) -> Option<(usize, Literal)> {
    let prefix = [&b"u8R\""[..], b"uR\"", b"UR\"", b"LR\"", b"R\""]
        .into_iter()
        .find(|p| rest.starts_with(p))?
        .len();
    let delim_len = rest[prefix..].iter().take(17).position(|&c| c == b'(')?;
    let delim = &rest[prefix..prefix + delim_len];
    if delim.iter().any(|c| c.is_ascii_whitespace() || matches!(c, b')' | b'\\')) {
        return None;
    }
    let mut close = vec![b')'];
    close.extend_from_slice(delim);
    close.push(b'"');
    Some((prefix + delim_len + 1, Literal::Raw { close }))
}

fn number_end(b: &[u8], i: usize, lang: Lang) -> usize {
    let mut j = i;
    while j < b.len() && (is_ident_byte(b[j]) || b[j] == b'.' || (lang == Lang::Cpp && b[j] == b'\'' && b.get(j + 1).is_some_and(u8::is_ascii_alphanumeric))) {
        j += 1;
    }
    j
}

fn ident_end(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| !is_ident_byte(c)).map_or(b.len(), |p| i + p)
}

pub(super) fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn directive_end(b: &[u8], mut i: usize) -> usize {
    loop {
        let end = line_end(b, i.min(b.len()));
        if end == b.len() || !b[..end].trim_ascii_end().ends_with(b"\\") {
            return end;
        }
        i = end + 1;
    }
}

fn directive_words(line: &[u8]) -> (&[u8], &[u8]) {
    let rest = line.trim_ascii_start().strip_prefix(b"#").unwrap_or_default().trim_ascii_start();
    let name = &rest[..ident_end(rest, 0)];
    let arg = rest[name.len()..].trim_ascii_start();
    (name, &arg[..ident_end(arg, 0)])
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p)
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| from + p)
}

fn opener_of(c: u8) -> u8 {
    match c {
        b')' => b'(',
        b']' => b'[',
        _ => b'{',
    }
}
