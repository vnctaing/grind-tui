use std::ops::Range;
use unicode_width::UnicodeWidthChar;

pub const TAB_WIDTH: usize = 4;

/// `col` counts bytes from the line start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRecord {
    pub at: usize,
    pub removed: String,
    pub inserted: String,
}

/// Every offset is a byte offset into `text` that lies on a char boundary.
pub struct Buffer {
    text: String,
    line_starts: Vec<usize>,
}

impl Buffer {
    pub fn new(text: &str) -> Buffer {
        let text = text.replace("\r\n", "\n");
        let line_starts = line_starts(&text);
        Buffer { text, line_starts }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The line's bytes, without its trailing newline.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let line = line.min(self.line_count() - 1);
        let start = self.line_starts[line];
        let end = self
            .line_starts
            .get(line + 1)
            .map_or(self.text.len(), |next| next - 1);
        start..end
    }

    pub fn line(&self, line: usize) -> &str {
        &self.text[self.line_range(line)]
    }

    pub fn pos(&self, offset: usize) -> Pos {
        let offset = offset.min(self.text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        Pos { line, col: offset - self.line_starts[line] }
    }

    pub fn offset(&self, pos: Pos) -> usize {
        let range = self.line_range(pos.line);
        let mut at = (range.start + pos.col).min(range.end);
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    pub fn edit(&mut self, range: Range<usize>, new: &str) -> EditRecord {
        let removed = self.text[range.clone()].to_string();
        self.text.replace_range(range.clone(), new);
        self.line_starts = line_starts(&self.text);
        EditRecord { at: range.start, removed, inserted: new.to_string() }
    }

    pub fn char_before(&self, offset: usize) -> Option<char> {
        self.text[..offset].chars().next_back()
    }

    pub fn char_after(&self, offset: usize) -> Option<char> {
        self.text[offset..].chars().next()
    }

    pub fn prev_boundary(&self, offset: usize) -> usize {
        self.char_before(offset).map_or(offset, |c| offset - c.len_utf8())
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.char_after(offset).map_or(offset, |c| offset + c.len_utf8())
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Terminal cells a char occupies when it starts at display column `col`.
pub fn char_cells(c: char, col: usize) -> usize {
    if c == '\t' {
        TAB_WIDTH - col % TAB_WIDTH
    } else {
        c.width().unwrap_or(0)
    }
}

pub fn display_col(line: &str, byte: usize) -> usize {
    line[..byte.min(line.len())]
        .chars()
        .fold(0, |col, c| col + char_cells(c, col))
}

/// The byte offset of the last char that starts at or before display column `col`.
pub fn byte_at_display(line: &str, col: usize) -> usize {
    let mut at = 0;
    for (i, c) in line.char_indices() {
        let next = at + char_cells(c, at);
        if next > col {
            return i;
        }
        at = next;
    }
    line.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_positions_round_trip_across_edits() {
        let mut b = Buffer::new("ab\ncd\n");
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line(1), "cd");
        assert_eq!(b.line(2), "");
        assert_eq!(b.pos(4), Pos { line: 1, col: 1 });
        assert_eq!(b.offset(Pos { line: 1, col: 9 }), 5);
        let rec = b.edit(1..4, "X\nY");
        assert_eq!(rec, EditRecord { at: 1, removed: "b\nc".into(), inserted: "X\nY".into() });
        assert_eq!(b.text(), "aX\nYd\n");
        assert_eq!(b.line(1), "Yd");
    }

    #[test]
    fn offsets_snap_to_char_boundaries() {
        let b = Buffer::new("é漢x");
        assert_eq!(b.offset(Pos { line: 0, col: 1 }), 0);
        assert_eq!(b.offset(Pos { line: 0, col: 4 }), 2);
        assert_eq!(b.next_boundary(2), 5);
        assert_eq!(b.prev_boundary(5), 2);
    }

    #[test]
    fn crlf_is_normalized() {
        assert_eq!(Buffer::new("a\r\nb").text(), "a\nb");
    }

    #[test]
    fn display_columns_expand_tabs_and_wide_chars() {
        assert_eq!(display_col("\tx", 1), 4);
        assert_eq!(display_col("ab\tx", 3), 4);
        assert_eq!(display_col("漢x", 3), 2);
        assert_eq!(byte_at_display("漢x", 1), 0);
        assert_eq!(byte_at_display("漢x", 2), 3);
        assert_eq!(byte_at_display("ab", 9), 2);
    }
}
