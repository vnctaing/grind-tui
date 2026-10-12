# Diagnostics

The editor checks the buffer as you type (tree-sitter syntax, bracket balance, a few lints) and shows the result in the gutter, the code panel's last row, the Status panel, and the help bar.

## Sub-features

- `diag-gutter` a `●` (error) or `▲` (warning) next to the line number.
- `diag-underline` the offending span is underlined in magenta (errors) or yellow (warnings).
- `diag-code-status` the code panel's last row: `● 1 error · Ln 3: Unclosed '['`, or `✓ no syntax errors`.
- `diag-summary` Status panel: `Syntax  ● 1 error · ▲ 0 warnings  F8 next`, or `✓ no errors`.
- `diag-f8` `F8` moves the caret to the next diagnostic (navigate or edit mode); the help bar gains `F8 next error`.

## How to get to it (user POV)

- Open a problem whose file has a syntax error, or type one in edit mode.

## Driving it with drive.py

Preconditions:

- Seed `.grind-tui/two-sum.py` with line 3 `        return [0, 1` (missing `]`) before `launch`.

- **Markers.** After opening: `expect ● 1 error`, `expect Ln 3: Unclosed '['`, `expect F8 next error`, `expect-ansi \x1b\[[0-9;:]*\b4[;:m]`.
- **F8.** `key F8`, `key Enter`. Title shows `Ln 3, Col 16`. Without F8 it shows `Ln 1, Col 1`.
- **Fix.** `key End`, `text ]`. `✓ no syntax errors`, no `● 1 error`; `Esc` saves it.

## Gotchas

- The code-panel status row shows the first diagnostic even before F8, so `Ln 3:` does not prove the jump. Assert the caret in edit mode.
- Underlines exist only in the `.ansi`; the PNG does not show them.
