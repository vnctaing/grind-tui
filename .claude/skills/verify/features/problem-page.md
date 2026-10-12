# Problem page

Opening a problem shows its description, a read-only Code panel for the JavaScript solution file, and a status panel with solved state, messages, and examples. The user edits the file in their own editor; grind-tui is the companion next to it.

## Sub-features

- `code-readonly` the Code panel shows the file with a line-number gutter (`1 │ /**`), JavaScript syntax colours (keywords purple), and the hint `read-only · c hide`. Typing into it changes nothing; `Enter` does nothing.
- `code-scroll` with Code focused, `j`/`k` scroll it a line.
- `panel-desc` `p` focuses, then collapses, then expands Description.
- `panel-output` `o` does the same for Status.
- `panel-code` `c` hides and shows Code.
- `panel-focus` `Tab` cycles focus Code, Description, Status.
- `seed` opening a problem seeds `<slug>.js` with its JavaScript starter.
- `legacy-state` a `state.json` from the multi-language version (with a `language` field) still loads; the field is dropped on the next save.
- `sync` `y` reloads the file from disk (`Synced.`).
- `yank` `Y` copies `nvim <path>` to the clipboard (`Copied: nvim …`).
- `solved` `x` toggles solved (`Marked solved.`).

## How to get to it (user POV)

- From the list, `Enter` on a row. `b` goes back, `q` quits.

## Driving it with drive.py

Preconditions:

- Seed `.grind-tui/state.json` with `{"solved":[1],"last_problem_id":1,"language":"rust"}`; `wait-for 1/167`, then open Two Sum with `key Enter`.

- **Read-only code.** `expect read-only · c hide`, `expect 1 │ /**`, `expect-ansi 38;2;185;103;255;1m?var` (keyword colour), `reject Enter edit`. `key Enter`, `text zzz`, then `reject zzz` and the file is still the starter.
- **Scroll.** `key j` hides `1 │ /**` and shows `2 │`; `key k` brings it back.
- **Focus.** `expect-ansi 48;2;255;42;109m Code`, then `key Tab` and the same pattern with `Description`, `Status`, `Code`.
- **Collapse.** `key p p` hides `You are given an array`; `key p` shows it. `key o o` hides `Examples`; `key c` hides `var twoSum`.
- **Sync and yank.** `key y` shows `Synced.`; `key Y` shows `Copied: nvim` and `expect-file clipboard.txt ~ two-sum.js`.
- **No language switch.** `key l` leaves `two-sum.js` on screen and never creates `two-sum.rs`.
- **Legacy state.** After quit, `reject-file .grind-tui/state.json ~ language` and `expect-file .grind-tui/two-sum.js ~ var twoSum = function(nums, target)`.

## Gotchas

- `p` and `o` change focus, so the next `Tab` starts from wherever they left it.
- `clipboard.txt` comes from the `pbcopy` stub; the real clipboard is never touched.
