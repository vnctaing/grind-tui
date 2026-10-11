use super::buffer::{Buffer, EditRecord};
use super::selection::Selection;
use std::time::{Duration, Instant};

pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

/// Only `Insert` and `Delete` runs merge; everything else is its own undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

struct Transaction {
    edits: Vec<EditRecord>,
    before: Selection,
    after: Selection,
    kind: EditKind,
    last_at: Instant,
}

#[derive(Default)]
pub struct UndoStack {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
}

impl UndoStack {
    /// Merges into the previous transaction when it is the same kind, the caret
    /// did not move in between, and it arrives within `GROUP_INTERVAL`.
    pub fn record(
        &mut self,
        edits: Vec<EditRecord>,
        before: Selection,
        after: Selection,
        kind: EditKind,
        now: Instant,
    ) {
        self.redo.clear();
        if let Some(top) = self.undo.last_mut()
            && kind != EditKind::Other
            && top.kind == kind
            && top.after == before
            && now.saturating_duration_since(top.last_at) < GROUP_INTERVAL
        {
            top.edits.extend(edits);
            top.after = after;
            top.last_at = now;
            return;
        }
        self.undo.push(Transaction { edits, before, after, kind, last_at: now });
    }

    pub fn undo(&mut self, buf: &mut Buffer) -> Option<Selection> {
        let tx = self.undo.pop()?;
        for e in tx.edits.iter().rev() {
            buf.edit(e.at..e.at + e.inserted.len(), &e.removed);
        }
        let sel = tx.before;
        self.redo.push(tx);
        Some(sel)
    }

    pub fn redo(&mut self, buf: &mut Buffer) -> Option<Selection> {
        let tx = self.redo.pop()?;
        for e in &tx.edits {
            buf.edit(e.at..e.at + e.removed.len(), &e.inserted);
        }
        let sel = tx.after;
        self.undo.push(tx);
        Some(sel)
    }
}
