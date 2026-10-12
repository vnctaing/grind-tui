# External editors

The problem page opens the solution file in an external editor from `~/.grind-tui/editors.json`. Blocking editors suspend the TUI and reload on exit; detached editors run in the background and the TUI auto-syncs on save.

## Sub-features

- `ext-blocking` `e` nvim, `v` vim: TUI suspends, editor runs in the same terminal, `Reloaded from vim.` on exit.
- `ext-detached` `C` Cursor, `V` VSCode, `Z` Zed: `Opened in Cursor, auto-syncing on save.`, then `Auto-synced from Cursor.` when the file changes.
- `ext-blocked` an untrusted command is refused: `Blocked: 'rm' is not a trusted editor`.

## How to get to it (user POV)

- Open a problem; the editor bar reads `open in e nvim • v vim • C Cursor • …`.

## Driving it with drive.py

Preconditions:

- Seed `.grind-tui/editors.json` with vim (`v`), Cursor (`C`, detach), and an untrusted `rm` (`X`) before `launch`.

- **Blocked.** `key X`. Status shows `Blocked: 'rm' is not a trusted editor`.
- **Vim.** `key v`, `wait-for two-sum.py` (vim's own status line), `key G o`, `text # edited in vim`, `key Escape`, `text :wq`, `key Enter`. `wait-for Reloaded from vim.`; code shows the line and the file contains it.
- **Cursor.** `key C`. `Opened in Cursor, auto-syncing on save.`; `wait-for Auto-synced from Cursor. ~ 6`; code shows `# saved by cursor stub`; `expect-file launched.txt ~ cursor`.

## Gotchas

- `cursor` is installed on this machine. Outside `drive.py` the `C` key would open the real app.
- vim runs with an empty HOME, so no vimrc; its screen is plain and has no theme colours.
- Auto-sync polls every 500 ms only on the problem page.
