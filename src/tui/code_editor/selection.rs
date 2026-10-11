use super::buffer::{byte_at_display, display_col, Buffer, Pos};
use std::ops::Range;

/// Byte offsets. Empty when `anchor == head`; `head` is where the caret sits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Selection {
        Selection { anchor: at, head: at }
    }

    pub fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

/// `goal_col` is the display column Up/Down try to return to across short lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub sel: Selection,
    pub goal_col: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    PageUp,
    PageDown,
    DocStart,
    DocEnd,
}

impl Cursor {
    pub fn set(&mut self, sel: Selection) {
        *self = Cursor { sel, goal_col: None };
    }

    pub fn apply(&mut self, buf: &Buffer, motion: Motion, extend: bool, page: usize) {
        let sel = self.sel;
        if !extend && !sel.is_empty() {
            match motion {
                Motion::Left => return self.set(Selection::caret(sel.range().start)),
                Motion::Right => return self.set(Selection::caret(sel.range().end)),
                _ => {}
            }
        }
        let (head, goal_col) = target(buf, self, motion, page);
        let anchor = if extend { sel.anchor } else { head };
        *self = Cursor { sel: Selection { anchor, head }, goal_col };
    }
}

fn target(buf: &Buffer, cursor: &Cursor, motion: Motion, page: usize) -> (usize, Option<usize>) {
    let head = cursor.sel.head;
    let pos = buf.pos(head);
    let vertical = |lines: isize| {
        let goal = cursor
            .goal_col
            .unwrap_or_else(|| display_col(buf.line(pos.line), pos.col));
        let last = buf.line_count() - 1;
        let line = pos.line.saturating_add_signed(lines).min(last);
        let at = match (lines < 0 && line == pos.line, lines > 0 && line == pos.line) {
            (true, _) => 0,
            (_, true) => buf.len(),
            _ => buf.offset(Pos { line, col: byte_at_display(buf.line(line), goal) }),
        };
        (at, Some(goal))
    };
    let page = page.max(1) as isize;
    match motion {
        Motion::Left => (buf.prev_boundary(head), None),
        Motion::Right => (buf.next_boundary(head), None),
        Motion::Up => vertical(-1),
        Motion::Down => vertical(1),
        Motion::PageUp => vertical(-page),
        Motion::PageDown => vertical(page),
        Motion::WordLeft => (word_left(buf.text(), head), None),
        Motion::WordRight => (word_right(buf.text(), head), None),
        Motion::Home => {
            let line = buf.line(pos.line);
            let indent = line.len() - line.trim_start().len();
            let col = if pos.col == indent { 0 } else { indent };
            (buf.offset(Pos { line: pos.line, col }), None)
        }
        Motion::End => (buf.line_range(pos.line).end, None),
        Motion::DocStart => (0, None),
        Motion::DocEnd => (buf.len(), None),
    }
}

#[derive(PartialEq)]
enum Class {
    Space,
    Word,
    Punct,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punct
    }
}

fn word_right(text: &str, from: usize) -> usize {
    let mut chars = text[from..].char_indices().skip_while(|&(_, c)| class(c) == Class::Space);
    let Some((_, first)) = chars.next() else { return text.len() };
    let kind = class(first);
    chars
        .find(|&(_, c)| class(c) != kind)
        .map_or(text.len(), |(i, _)| from + i)
}

fn word_left(text: &str, from: usize) -> usize {
    let mut chars = text[..from].char_indices().rev().skip_while(|&(_, c)| class(c) == Class::Space);
    let Some((_, first)) = chars.next() else { return 0 };
    let kind = class(first);
    chars
        .find(|&(_, c)| class(c) != kind)
        .map_or(0, |(i, c)| i + c.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(text: &str, at: usize, motion: Motion) -> usize {
        let buf = Buffer::new(text);
        let mut c = Cursor::default();
        c.set(Selection::caret(at));
        c.apply(&buf, motion, false, 10);
        c.sel.head
    }

    #[test]
    fn word_jumps_stop_at_class_changes() {
        let text = "let foo_bar = x.y;";
        assert_eq!(moved(text, 0, Motion::WordRight), 3);
        assert_eq!(moved(text, 3, Motion::WordRight), 11);
        assert_eq!(moved(text, 11, Motion::WordRight), 13);
        assert_eq!(moved(text, 15, Motion::WordRight), 16);
        assert_eq!(moved(text, 11, Motion::WordLeft), 4);
        assert_eq!(moved(text, 4, Motion::WordLeft), 0);
        assert_eq!(moved(text, 18, Motion::WordLeft), 17);
        assert_eq!(moved("a  \n  b", 1, Motion::WordRight), 7);
    }

    #[test]
    fn word_jumps_step_over_multibyte_words() {
        assert_eq!(moved("héllo wörld", 0, Motion::WordRight), 6);
        assert_eq!(moved("héllo wörld", 12, Motion::WordLeft), 7);
    }

    #[test]
    fn arrows_step_whole_chars() {
        assert_eq!(moved("é漢", 0, Motion::Right), 2);
        assert_eq!(moved("é漢", 5, Motion::Left), 2);
    }

    #[test]
    fn vertical_moves_keep_the_goal_column() {
        let buf = Buffer::new("abcdef\nab\nabcdef");
        let mut c = Cursor::default();
        c.set(Selection::caret(5));
        c.apply(&buf, Motion::Down, false, 10);
        assert_eq!(buf.pos(c.sel.head), Pos { line: 1, col: 2 });
        c.apply(&buf, Motion::Down, false, 10);
        assert_eq!(buf.pos(c.sel.head), Pos { line: 2, col: 5 });
        c.apply(&buf, Motion::Down, false, 10);
        assert_eq!(c.sel.head, buf.len());
    }

    #[test]
    fn home_toggles_between_indent_and_column_zero() {
        assert_eq!(moved("    x", 5, Motion::Home), 4);
        assert_eq!(moved("    x", 4, Motion::Home), 0);
    }

    #[test]
    fn shift_extends_and_plain_arrow_collapses() {
        let buf = Buffer::new("hello");
        let mut c = Cursor::default();
        c.apply(&buf, Motion::Right, true, 1);
        c.apply(&buf, Motion::Right, true, 1);
        assert_eq!(c.sel, Selection { anchor: 0, head: 2 });
        c.apply(&buf, Motion::Left, false, 1);
        assert_eq!(c.sel, Selection::caret(0));
    }
}
