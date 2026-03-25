# grind-tui

A terminal UI for LeetCode built with Rust. Fetch problems, browse with search and filters, open solutions in your editor, run tests, and submit—all without leaving the terminal.

<img src="https://github.com/user-attachments/assets/029edfa6-7dfb-47f2-9a98-8e0155312ffd" alt="Demo of the TUI" width="350">

## Features

- Browse all LeetCode problems with real-time filtering by difficulty, title, or ID
- Open problems in your editor (nvim, vim, Cursor, VSCode) with auto-sync on save
- Run test cases and submit solutions directly from the TUI
- Persists language preference, solved problems, and last opened problem
- Multi-language support: python3, rust, javascript, typescript, java, cpp, go
- Color-coded output with element-level diff highlighting on test failures

## Install

### Homebrew (recommended)

```bash
brew tap vnctaing/tap
brew install grind-tui
```

### Cargo

```bash
cargo install grind-tui
```

### Build from source

```bash
git clone https://github.com/vnctaing/grind-tui.git
cd grind-tui
cargo build --release
sudo cp target/release/grind /usr/local/bin/grind
```

## Setup

Run setup once to authenticate with LeetCode:

```bash
grind setup
```

Follow the prompts to:
1. Log into LeetCode in Chrome
2. Open DevTools (Cmd+Option+I)
3. Navigate to Application → Cookies → https://leetcode.com
4. Copy your `LEETCODE_SESSION` and `csrftoken` values

Credentials are saved to `~/.grind-tui/session.json`.

## Usage

Launch the TUI:

```bash
grind
```

Filter by difficulty:

```bash
grind --filter easy
grind --filter medium
grind --filter hard
```

Logout (removes saved credentials):

```bash
grind logout
```

## Keybindings

**List Page:**
- `j`/`↓` — Move down
- `k`/`↑` — Move up
- `/` — Search by title or problem ID
- `Enter` — Open problem
- `q` — Quit

**Problem Page:**
- `Tab` — Cycle through panels (description, code, output)
- `e`, `v`, `C`, `V` — Open in nvim, vim, Cursor, or VSCode
- `y` — Force sync from detached editor
- `Y` — Copy command to open in new editor tab
- `r` — Run test cases
- `s` — Submit solution
- `c` — Collapse/expand code panel
- `o` — Collapse/expand output panel
- `p` — Toggle problem description collapse
- `l` — Change language
- `j`/`k` — Scroll within focused panel
- `b` — Back to list
- `q` — Quit

## Config

Editor bindings and other settings are stored in `~/.grind-tui/`. Customize editor commands by editing `~/.grind-tui/editors.json`:

```json
[
  {"name": "nvim", "key": "e", "command": "nvim", "detach": false},
  {"name": "vim", "key": "v", "command": "vim", "detach": false},
  {"name": "Cursor", "key": "C", "command": "cursor", "detach": true},
  {"name": "VSCode", "key": "V", "command": "code", "detach": true}
]
```

Set `detach: true` for editors that spawn asynchronously (Cursor, VSCode). When detached, the TUI watches your file for changes and auto-syncs.

## Requirements

- Active LeetCode account
- Cookie access from Chrome DevTools

## License

MIT
