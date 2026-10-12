# Problem page

Opening a problem shows its description, the code panel for the current language, and a status panel with solved state, syntax summary, messages, and examples.

## Sub-features

- `panel-desc` `p` focuses, then collapses, then expands Description.
- `panel-output` `o` does the same for Status.
- `panel-code` `c` hides and shows Code.
- `panel-focus` `Tab` cycles focus Code, Description, Status.
- `lang` `l` cycles Python, Rust, JavaScript, TypeScript, Java, C++, Go, C and seeds `<slug>.<ext>`.
- `sync` `y` reloads the file from disk (`Synced.`); twice when the buffer is dirty.
- `yank` `Y` copies `nvim <path>` to the clipboard (`Copied: nvim …`).
- `solved` `x` toggles solved (`Marked solved.`).

## How to get to it (user POV)

- From the list, `Enter` on a row. `b` goes back, `q` quits.

## Driving it with drive.py

Preconditions:

- Fresh HOME; open Two Sum with `key Enter`.

- **Focus.** `expect-ansi 48;2;255;42;109m Code`, then `key Tab` and the same pattern with `Description`, `Status`, `Code`.
- **Collapse.** `key p p` hides `You are given an array`; `key p` shows it. `key o o` hides `Examples`; `key c` hides `class Solution`.
- **Sync and yank.** `key y` shows `Synced.`; `key Y` shows `Copied: nvim` and `expect-file clipboard.txt ~ two-sum.py`.
- **Language.** `key l` shows `l Rust`, `two-sum.rs`, `impl Solution`; after quit `state.json` contains `rust` and `.grind-tui/two-sum.rs` exists.

## Gotchas

- `p` and `o` change focus, so the next `Tab` starts from wherever they left it.
- `clipboard.txt` comes from the `pbcopy` stub; the real clipboard is never touched.
- `q`, `b`, `l`, and editor keys are refused while the buffer is dirty (`Unsaved edits. …`).
