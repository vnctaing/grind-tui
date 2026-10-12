use super::{Diagnostic, Source};

pub struct Scan {
    pub blank: Vec<u8>,
    pub diagnostics: Vec<Diagnostic>,
    pub unclosed: Vec<Unclosed>,
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

pub fn scan(text: &str) -> Scan {
    let mut s = Scanner {
        bytes: text.as_bytes(),
        blank: text.as_bytes().to_vec(),
        stack: Vec::new(),
        diagnostics: Vec::new(),
        unclosed: Vec::new(),
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
    Scan { blank: s.blank, diagnostics: s.diagnostics, unclosed: s.unclosed }
}

struct Scanner<'a> {
    bytes: &'a [u8],
    blank: Vec<u8>,
    stack: Vec<Frame>,
    diagnostics: Vec<Diagnostic>,
    unclosed: Vec<Unclosed>,
    /// A literal or comment ran to the end of the text, so the brackets still
    /// open were most likely swallowed by it rather than left open.
    swallowed: bool,
    regex_allowed: bool,
}

impl Scanner<'_> {
    fn run(&mut self, mut i: usize) {
        let b = self.bytes;
        while i < b.len() {
            let c = b[i];
            let rest = &b[i..];
            let was_regex_allowed = self.regex_allowed;
            self.regex_allowed = false;
            if rest.starts_with(b"//") {
                let end = line_end(b, i);
                self.erase_comment(i, end);
                i = end;
                self.regex_allowed = was_regex_allowed;
            } else if rest.starts_with(b"/*") {
                i = self.block_comment(i);
                self.regex_allowed = was_regex_allowed;
            } else if matches!(c, b'"' | b'\'') {
                i = self.string(i);
            } else if c == b'`' {
                i = self.template(i, i + 1);
            } else if c == b'/' && was_regex_allowed {
                i = self.regex(i);
            } else if is_ident_byte(c) && !c.is_ascii_digit() {
                let end = ident_end(b, i);
                self.regex_allowed = REGEX_KEYWORDS.contains(&&b[i..end]);
                i = end;
            } else if c.is_ascii_digit() {
                i = number_end(b, i);
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

    fn opens_statement_header(&self, open: usize) -> bool {
        let end = self.blank[..open].iter().rposition(|c| !c.is_ascii_whitespace()).map_or(0, |p| p + 1);
        let start = self.blank[..end].iter().rposition(|&c| !is_ident_byte(c)).map_or(0, |p| p + 1);
        matches!(&self.blank[start..end], b"if" | b"while" | b"for" | b"with")
    }

    fn string(&mut self, start: usize) -> usize {
        let b = self.bytes;
        let quote = b[start];
        let mut j = start + 1;
        while j < b.len() && b[j] != b'\n' {
            if b[j] == b'\\' {
                j += 2;
            } else if b[j] == quote {
                self.erase(start, j + 1);
                return j + 1;
            } else {
                j += 1;
            }
        }
        let j = j.min(b.len());
        self.diagnostics.push(unterminated(start, b, "string"));
        self.swallowed |= j == b.len();
        self.erase(start, j);
        j
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
        if let Some(end) = find(b, start + 2, b"*/") {
            self.erase_comment(start, end + 2);
            return end + 2;
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

fn number_end(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| !is_ident_byte(c) && c != b'.').map_or(b.len(), |p| i + p)
}

fn ident_end(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| !is_ident_byte(c)).map_or(b.len(), |p| i + p)
}

pub(super) fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
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
