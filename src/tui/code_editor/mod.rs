mod buffer;
mod history;
mod render;
mod selection;

use super::highlight::quotes;
use crate::catalog::Lang;
use buffer::{display_col, Buffer, EditRecord, TAB_WIDTH};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use history::{EditKind, UndoStack};
use ratatui::layout::Rect;
use selection::{Cursor, Motion, Selection};
use std::borrow::Cow;
use std::cell::Cell;
use std::ops::Range;
use std::time::Instant;

/// What the caller must do after a key, since the editor itself never touches
/// the disk or the system clipboard.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Save,
    Copy(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Insert(char),
    Newline,
    Indent,
    Outdent,
    Backspace,
    Delete,
    Move(Motion, bool),
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    Save,
}

fn command(key: KeyEvent) -> Option<Command> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let word = ctrl || alt;
    let mv = |m| Some(Command::Move(m, shift));
    match key.code {
        KeyCode::Char(c) if ctrl && alt => Some(Command::Insert(c)),
        KeyCode::Char(c) if ctrl => match c {
            'a' | 'A' => Some(Command::SelectAll),
            'c' | 'C' => Some(Command::Copy),
            'x' | 'X' => Some(Command::Cut),
            'v' | 'V' => Some(Command::Paste),
            'z' if !shift => Some(Command::Undo),
            'z' | 'Z' | 'y' | 'Y' => Some(Command::Redo),
            's' | 'S' => Some(Command::Save),
            _ => None,
        },
        KeyCode::Char('b') if alt => mv(Motion::WordLeft),
        KeyCode::Char('f') if alt => mv(Motion::WordRight),
        KeyCode::Char(c) => Some(Command::Insert(c)),
        KeyCode::Enter => Some(Command::Newline),
        KeyCode::Tab => Some(Command::Indent),
        KeyCode::BackTab => Some(Command::Outdent),
        KeyCode::Backspace => Some(Command::Backspace),
        KeyCode::Delete => Some(Command::Delete),
        KeyCode::Left if word => mv(Motion::WordLeft),
        KeyCode::Right if word => mv(Motion::WordRight),
        KeyCode::Left => mv(Motion::Left),
        KeyCode::Right => mv(Motion::Right),
        KeyCode::Up => mv(Motion::Up),
        KeyCode::Down => mv(Motion::Down),
        KeyCode::Home if ctrl => mv(Motion::DocStart),
        KeyCode::End if ctrl => mv(Motion::DocEnd),
        KeyCode::Home => mv(Motion::Home),
        KeyCode::End => mv(Motion::End),
        KeyCode::PageUp => mv(Motion::PageUp),
        KeyCode::PageDown => mv(Motion::PageDown),
        _ => None,
    }
}

/// `whole_line` clips came from a copy or cut with no selection, and paste
/// them above the caret's line, as VS Code does.
#[derive(Default)]
struct Clip {
    text: String,
    whole_line: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Scroll {
    top: usize,
    left: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Viewport {
    rows: usize,
    cols: usize,
}

pub struct CodeEditor {
    buffer: Buffer,
    cursor: Cursor,
    history: UndoStack,
    clip: Clip,
    saved: String,
    lang: Lang,
    scroll: Scroll,
    /// Text area size from the last frame, for paging and keeping the caret in view.
    viewport: Cell<Viewport>,
}

impl CodeEditor {
    pub fn new(text: &str, lang: Lang) -> CodeEditor {
        let buffer = Buffer::new(text);
        CodeEditor {
            saved: buffer.text().to_string(),
            buffer,
            cursor: Cursor::default(),
            history: UndoStack::default(),
            clip: Clip::default(),
            lang,
            scroll: Scroll::default(),
            viewport: Cell::default(),
        }
    }

    #[cfg(test)]
    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// The text as it should be written to disk, in the file's own line endings.
    pub fn contents(&self) -> Cow<'_, str> {
        self.buffer.contents()
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.text() != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.buffer.text().to_string();
    }

    /// Replaces the text from disk. Undo history is dropped because it
    /// describes a text that no longer exists.
    pub fn reload(&mut self, text: &str) {
        let head = self.cursor.sel.head;
        self.buffer = Buffer::new(text);
        let pos = self.buffer.pos(head);
        self.cursor.set(Selection::caret(self.buffer.offset(pos)));
        self.history = UndoStack::default();
        self.mark_saved();
        self.scroll.top = self.scroll.top.min(self.buffer.line_count() - 1);
    }

    /// One-based line and display column of the caret.
    pub fn caret(&self) -> (usize, usize) {
        let pos = self.buffer.pos(self.cursor.sel.head);
        (pos.line + 1, display_col(self.buffer.line(pos.line), pos.col) + 1)
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) -> Effect {
        let Some(cmd) = command(key) else { return Effect::None };
        let effect = self.run(cmd, now);
        self.reveal();
        effect
    }

    pub fn paste(&mut self, text: &str, now: Instant) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let range = self.cursor.sel.range();
        self.edit(range.clone(), &text, range.start + text.len(), EditKind::Other, now);
        self.reveal();
    }

    pub fn scroll_by(&mut self, delta: isize) {
        let last = self.buffer.line_count() - 1;
        self.scroll.top = self.scroll.top.saturating_add_signed(delta).min(last);
    }

    pub fn render(&self, area: Rect, out: &mut ratatui::buffer::Buffer, show_cursor: bool) {
        render::render(self, area, out, show_cursor);
    }

    fn run(&mut self, cmd: Command, now: Instant) -> Effect {
        match cmd {
            Command::Insert(c) => self.type_char(c, now),
            Command::Newline => self.newline(now),
            Command::Indent => self.indent(now),
            Command::Outdent => self.edit_line_starts(|line| outdent_len(line), "", now),
            Command::Backspace => self.backspace(now),
            Command::Delete => self.delete_forward(now),
            Command::Move(motion, extend) => {
                let page = self.viewport.get().rows.saturating_sub(1);
                self.cursor.apply(&self.buffer, motion, extend, page);
            }
            Command::SelectAll => self.cursor.set(Selection { anchor: 0, head: self.buffer.len() }),
            Command::Copy => return self.copy(),
            Command::Cut => return self.cut(now),
            Command::Paste => self.paste_clip(now),
            Command::Undo => {
                if let Some(sel) = self.history.undo(&mut self.buffer) {
                    self.cursor.set(sel);
                }
            }
            Command::Redo => {
                if let Some(sel) = self.history.redo(&mut self.buffer) {
                    self.cursor.set(sel);
                }
            }
            Command::Save => return Effect::Save,
        }
        Effect::None
    }

    fn edit(&mut self, range: Range<usize>, text: &str, caret: usize, kind: EditKind, now: Instant) {
        let before = self.cursor.sel;
        let rec = self.buffer.edit(range, text);
        let after = Selection::caret(caret);
        self.history.record(vec![rec], before, after, kind, now);
        self.cursor.set(after);
    }

    fn type_char(&mut self, c: char, now: Instant) {
        let sel = self.cursor.sel;
        let range = sel.range();
        let prev = self.buffer.char_before(range.start);
        let next = self.buffer.char_after(range.end);
        let quote = quotes(self.lang).contains(&c);
        if sel.is_empty() && next == Some(c) && (quote || is_closer(c)) {
            return self.cursor.set(Selection::caret(range.end + c.len_utf8()));
        }
        let closes_cleanly = next.is_none_or(|n| n.is_whitespace() || is_closer(n));
        let close = match c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            _ if quote && !prev.is_some_and(|p| p.is_alphanumeric() || p == c) => Some(c),
            _ => None,
        }
        .filter(|_| sel.is_empty() && closes_cleanly);
        let text: String = std::iter::once(c).chain(close).collect();
        self.edit(range.clone(), &text, range.start + c.len_utf8(), EditKind::Insert, now);
    }

    fn newline(&mut self, now: Instant) {
        let range = self.cursor.sel.range();
        let pos = self.buffer.pos(range.start);
        let line = self.buffer.line(pos.line);
        let before_caret = &line[..pos.col];
        let indent = &before_caret[..before_caret.len() - before_caret.trim_start().len()];
        let opener = self.buffer.char_before(range.start);
        let opens = matches!(opener, Some('(' | '[' | '{'))
            || (self.lang == Lang::Python3 && opener == Some(':'));
        let mut text = format!("\n{indent}");
        if opens {
            text.push_str(indent_unit(self.lang));
        }
        let caret = range.start + text.len();
        let next = self.buffer.char_after(range.end);
        if opens && next.is_some() && next == opener.and_then(closer_of) {
            text.push('\n');
            text.push_str(indent);
        }
        self.edit(range, &text, caret, EditKind::Other, now);
    }

    fn indent(&mut self, now: Instant) {
        let sel = self.cursor.sel;
        let range = sel.range();
        if self.buffer.pos(range.start).line != self.buffer.pos(range.end).line {
            return self.edit_line_starts(|_| 0, indent_unit(self.lang), now);
        }
        let text = match self.lang {
            Lang::Go => "\t".to_string(),
            _ => {
                let pos = self.buffer.pos(range.start);
                let col = display_col(self.buffer.line(pos.line), pos.col);
                " ".repeat(TAB_WIDTH - col % TAB_WIDTH)
            }
        };
        self.edit(range.clone(), &text, range.start + text.len(), EditKind::Other, now);
    }

    /// Removes `remove(line)` bytes then inserts `insert` at the start of every
    /// line the selection touches, as one undo step.
    fn edit_line_starts(&mut self, remove: impl Fn(&str) -> usize, insert: &str, now: Instant) {
        let before = self.cursor.sel;
        let mut sel = before;
        let mut edits = Vec::new();
        for line in self.touched_lines().rev() {
            let start = self.buffer.line_range(line).start;
            let text = self.buffer.line(line);
            let len = remove(text);
            if len == 0 && (insert.is_empty() || text.is_empty()) {
                continue;
            }
            let rec = self.buffer.edit(start..start + len, insert);
            sel = Selection { anchor: map_through(sel.anchor, &rec), head: map_through(sel.head, &rec) };
            edits.push(rec);
        }
        if !edits.is_empty() {
            self.history.record(edits, before, sel, EditKind::Other, now);
            self.cursor.set(sel);
        }
    }

    fn touched_lines(&self) -> Range<usize> {
        let range = self.cursor.sel.range();
        let first = self.buffer.pos(range.start).line;
        let end = self.buffer.pos(range.end);
        let last = if end.col == 0 && end.line > first { end.line - 1 } else { end.line };
        first..last + 1
    }

    fn backspace(&mut self, now: Instant) {
        let sel = self.cursor.sel;
        if !sel.is_empty() {
            let range = sel.range();
            return self.edit(range.clone(), "", range.start, EditKind::Delete, now);
        }
        let head = sel.head;
        if head == 0 {
            return;
        }
        let pos = self.buffer.pos(head);
        let before_caret = &self.buffer.line(pos.line)[..pos.col];
        let prev = self.buffer.char_before(head);
        let next = self.buffer.char_after(head);
        let start = if !before_caret.is_empty() && before_caret.bytes().all(|b| b == b' ') {
            let stop = (pos.col - 1) / TAB_WIDTH * TAB_WIDTH;
            head - (pos.col - stop)
        } else {
            self.buffer.prev_boundary(head)
        };
        let paired = prev.is_some_and(|p| {
            next.is_some() && (next == closer_of(p) || (quotes(self.lang).contains(&p) && next == Some(p)))
        });
        let end = if paired { self.buffer.next_boundary(head) } else { head };
        self.edit(start..end, "", start, EditKind::Delete, now);
    }

    fn delete_forward(&mut self, now: Instant) {
        let range = self.cursor.sel.range();
        let end = if range.is_empty() { self.buffer.next_boundary(range.end) } else { range.end };
        if end > range.start {
            self.edit(range.start..end, "", range.start, EditKind::Delete, now);
        }
    }

    /// The selection, or the caret's whole line when nothing is selected.
    /// A blank line with no selection has nothing worth copying.
    fn clip_range(&self) -> Option<(Range<usize>, Clip)> {
        let sel = self.cursor.sel;
        if !sel.is_empty() {
            let range = sel.range();
            let text = self.buffer.text()[range.clone()].to_string();
            return Some((range, Clip { text, whole_line: false }));
        }
        let line = self.buffer.pos(sel.head).line;
        let range = self.buffer.line_range(line);
        let text = &self.buffer.text()[range.clone()];
        if text.trim().is_empty() {
            return None;
        }
        let text = format!("{text}\n");
        let with_newline = range.start..(range.end + 1).min(self.buffer.len());
        Some((with_newline, Clip { text, whole_line: true }))
    }

    fn copy(&mut self) -> Effect {
        let Some((_, clip)) = self.clip_range() else { return Effect::None };
        let text = clip.text.clone();
        self.clip = clip;
        Effect::Copy(text)
    }

    fn cut(&mut self, now: Instant) -> Effect {
        let Some((range, clip)) = self.clip_range() else { return Effect::None };
        let text = clip.text.clone();
        self.clip = clip;
        self.edit(range.clone(), "", range.start, EditKind::Other, now);
        Effect::Copy(text)
    }

    fn paste_clip(&mut self, now: Instant) {
        let text = self.clip.text.clone();
        if self.clip.whole_line && self.cursor.sel.is_empty() {
            let head = self.cursor.sel.head;
            let start = self.buffer.line_range(self.buffer.pos(head).line).start;
            return self.edit(start..start, &text, head + text.len(), EditKind::Other, now);
        }
        let range = self.cursor.sel.range();
        self.edit(range.clone(), &text, range.start + text.len(), EditKind::Other, now);
    }

    fn reveal(&mut self) {
        let vp = self.viewport.get();
        if vp.rows == 0 || vp.cols == 0 {
            return;
        }
        let pos = self.buffer.pos(self.cursor.sel.head);
        let col = display_col(self.buffer.line(pos.line), pos.col);
        let s = &mut self.scroll;
        s.top = s.top.clamp(pos.line.saturating_sub(vp.rows - 1), pos.line);
        s.left = s.left.clamp(col.saturating_sub(vp.cols - 1), col);
    }
}

fn indent_unit(lang: Lang) -> &'static str {
    match lang {
        Lang::Go => "\t",
        _ => "    ",
    }
}

fn outdent_len(line: &str) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    line.bytes().take(TAB_WIDTH).take_while(|&b| b == b' ').count()
}

fn is_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

fn closer_of(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// Where offset `p` lands after `rec` is applied.
fn map_through(p: usize, rec: &EditRecord) -> usize {
    if p < rec.at {
        p
    } else if p >= rec.at + rec.removed.len() {
        p - rec.removed.len() + rec.inserted.len()
    } else {
        rec.at
    }
}

#[cfg(test)]
mod tests;
