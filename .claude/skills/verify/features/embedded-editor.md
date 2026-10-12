# Embedded editor

The Code panel is an editor. `Enter` switches to edit mode where keys type text, with undo/redo, clipboard, save to disk, reload, and discard.

## Sub-features

- `edit-enter` `Enter` enters edit mode (`Esc done` in the panel title and help bar).
- `edit-type` typing marks the buffer dirty (`● unsaved`).
- `edit-undo` `C-z` undoes a typing group, `C-y` redoes.
- `edit-save` `C-s` writes the file (`Saved /…/two-sum.py.`).
- `edit-esc` `Esc` saves if dirty and returns to navigate mode (`Enter edit`).
- `edit-reload` `C-r` reloads from disk; twice when dirty (`press Ctrl-R again`).
- `edit-discard` `C-q` twice drops unsaved edits and leaves edit mode (`Discarded unsaved changes.`).
- `edit-conflict` an external write while dirty shows `File changed on disk while you have unsaved edits.` Not in the canonical scenario; drive it with a `write .grind-tui/two-sum.py` step after typing.

## How to get to it (user POV)

- Open a problem, press `Enter`.

## Driving it with drive.py

Preconditions:

- Fresh HOME; open Two Sum.

- **Type.** `key Enter`, `key C-End`, `text return [7, 7]`. Shows `● unsaved` and `return [7, 7]`.
- **Undo/redo.** `key C-z` removes `return [7, 7]`; `key C-y` restores it.
- **Save.** `key C-s`, `wait-for Saved`, `reject ● unsaved`, `expect-file .grind-tui/two-sum.py ~ return [7, 7]`.
- **Reload.** `text  # scratch`, `key C-r` (`press Ctrl-R`), `key C-r` (`Reloaded from disk.`), `reject # scratch`.
- **Discard.** `text  # doomed`, `key C-q`, `key C-q`. `Discarded unsaved changes.`, `Enter edit`, and `reject-file .grind-tui/two-sum.py ~ doomed`.

## Gotchas

- The Python starter's last line is already indented 8 spaces, so `C-End` lands at column 9; do not type indentation.
- `tmux send-keys -l` types a whole `text` step inside one undo group.
- Any other key between two `C-r` or `C-q` presses disarms the confirmation.
