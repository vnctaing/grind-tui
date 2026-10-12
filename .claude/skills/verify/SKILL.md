---
name: verify
description: Drive the real grind-tui binary (`grind`, a Rust/ratatui TUI) like a user in an isolated tmux server with a temp HOME and no network, and produce per-step PNG/ANSI/text proof plus file side-effect checks. Use whenever you change grind-tui behavior or rendering, review someone else's change, or reproduce a TUI bug before fixing it.
---

# Verify grind-tui

The surface is one terminal app, `target/release/grind`. It has no server, no login, and no network: the catalog is embedded. All state lives in `$HOME/.grind-tui/` (`state.json`, `editors.json`, one JavaScript solution file per problem such as `two-sum.js`). A verification run is a scenario file of keystrokes and assertions, executed by `scripts/drive.py`.

Never run the binary against your real HOME. `drive.py` always uses a fresh temp HOME, so the real `~/.grind-tui` is never read or written.

## Launch

Nothing stays running between runs. `drive.py` does all of this per run:

1. `cargo build --release` in the repo root (a no-op when current).
2. A fresh temp HOME (`$TMPDIR/grind-verify-home-*`) with `stub-bin/` first on PATH. The stubs shadow `pbcopy` (writes `$HOME/clipboard.txt`, so the host clipboard is untouched) and the GUI editors `code`, `cursor`, `zed` (record the call in `$HOME/launched.txt`, then append `// saved by <name> stub` to the file after 1 s, like a detached editor saving). `vim` and `nvim` are the real ones.
3. A private tmux server `tmux -L grind-verify-<pid> -f /dev/null` with `terminal-features ',*:RGB'`, `remain-on-exit on`, and `escape-time 0`.
4. The app in session `g` at the requested size, as `env -i HOME=… PATH=… TERM=xterm-256color sandbox-exec -p '(version 1)(allow default)(deny network*)' target/release/grind [args]`.

Ready means the pane is non-blank and two captures 150 ms apart match. Every `key`, `text`, and `launch` step waits for that before capturing.

## Doctor

```bash
.claude/skills/verify/scripts/doctor.sh
```

Read-only. It checks that `tmux`, `freeze`, `sandbox-exec`, `cargo`, and `python3` are on PATH, that the release binary is newer than `src/`, and that no `grind-verify-*` tmux socket or temp HOME is stranded. Run it first whenever a run behaves oddly.

## Drive

```bash
python3 -I .claude/skills/verify/scripts/drive.py SCENARIO.scn [--size 80x24] [--out DIR] [--keep]
.claude/skills/verify/scripts/run_all.sh [name]   # every scenario (or scenarios/<name>.scn) at 140x40 and 80x24
```

`--size` overrides the scenario's `size` line (default 140x40). `--out` defaults to `target/verify/<scenario>-<WxH>/`, wiped at the start of each run. `--keep` leaves the tmux server and temp HOME up and prints `tmux -L <sock> attach -t g` so you can look by hand; run `cleanup.sh` afterwards. `drive.py` exits 1 if any assertion failed. `run_all.sh` writes `target/verify/SUMMARY.txt`; `SIZES="100x30" run_all.sh` changes the matrix.

Scenario steps, one per line (`#` lines are comments):

| Step | Meaning |
| --- | --- |
| `size WxH` | Default size for this scenario. |
| `write REL` … `EOF` | Write the following lines to `$HOME/REL`. Before `launch` it seeds state; after, it is an external edit. |
| `launch [args]` | Start the app, e.g. `launch --list grind75 --difficulty easy`. |
| `key K [K…]` | tmux key names, sent one by one: `Enter`, `Esc`, `Tab`, `BTab`, `Up`, `End`, `C-End`, `C-r`, `C-s`, `F8`, `BSpace`, or a single character such as `x` or `G`. |
| `text …` | Literal text, everything after `text ` including leading spaces. Typed into whatever has focus. |
| `sleep S` | Wait S seconds, then capture. |
| `resize WxH` | Resize the window, then capture. |
| `wait-for TEXT [~ SECONDS]` | Poll until TEXT is on screen (default 5 s); fails if it never shows. |
| `snap NAME` | Capture and render `NN-NAME.png`. |
| `expect TEXT` / `reject TEXT` | TEXT is / is not in the last capture's plain text. |
| `expect-ansi REGEX` | Python regex over the last raw ANSI capture, e.g. `\x1b\[[0-9;:]*\b4[;:m]` (underline) or `48;2;255;42;109m Code` (Code is the focused panel). |
| `expect-file REL [~ TEXT]` | `$HOME/REL` exists and contains TEXT. |
| `reject-file REL [~ TEXT]` | `$HOME/REL` is absent, or lacks TEXT. |
| `expect-exit CODE` | The app exited with CODE (pane is dead). |

Stable handles to assert on are the help bar labels (`Enter edit`, `Esc done`, `F8 next error`), the status messages from `src/tui/app.rs` (`Saved …`, `Reloaded from disk.`, `Discarded unsaved changes.`, `Synced.`, `Auto-synced from Cursor.`), the counters (`167 problems`, `0/167`), and the editor bar path (`two-sum.js`). Pick strings that only the new state can produce: `return [0, 1]` appears in the Two Sum description, so it proves nothing about the editor.

## Evidence

Per run, in `target/verify/<scenario>-<WxH>/`:

- `NN-<step>.txt`. The plain screen after each step, for grep.
- `NN-<step>.ansi`. The same screen with escapes, captured one row at a time (`capture-pane -p -e -N -S r -E r`, joined). Grep it for colours and underlines; `freeze` does not draw underlines.
- `NN-<name>.png`. For `snap` steps, `freeze --language ansi --output x.png < x.ansi`.
- `home/`. A copy of the temp HOME's `.grind-tui/` plus `clipboard.txt` and `launched.txt`, taken at teardown.
- `report.txt`. Every step with PASS/FAIL and the final `RESULT`.

Proof standards:

- Drive the real path a user takes: keys into the real binary. No test hooks.
- Capture the action and the resulting state: a `snap` before and after the key that matters.
- Check side effects on disk with `expect-file` (Ctrl-S writes the solution file, `x` and quit write `state.json`), not only the screen.
- Look at the PNGs before reporting. A passing grep can still sit on a wrong screen.
- Make an assertion fail once on purpose (drop the key it depends on) before trusting a new one.

## Cleanup

`drive.py` kills its own tmux server, deletes its socket file, and removes its temp HOME on exit, including on failure. Evidence in `target/verify/` stays. After an interrupted run or `--keep`:

```bash
.claude/skills/verify/scripts/cleanup.sh
```

It only touches `grind-verify-*` sockets and `grind-verify-home-*` dirs, which only `drive.py` creates. Runs are isolated by socket and HOME, so several can run in parallel; give each a distinct `--out`.

## Feature map

[`features/README.md`](features/README.md) lists each user-facing feature, how to reach it, and its canonical scenario in `scenarios/`. A change to a feature is verified by its scenario at both sizes plus any new steps the change needs.

## Gotchas

- Whole-pane `capture-pane -e` writes each line's colours relative to the previous line and drops trailing background; `freeze` then draws white borders and grey margins. Those are capture artifacts, not app bugs. `drive.py` captures per row for this reason.
- `freeze file.ansi` (file as argument) hangs when non-interactive. Always feed stdin.
- `freeze`'s font has no `↻` glyph, so the detached-editor bar shows a box in PNGs. The `.txt` has the real character.
- At 80x24 the help bar drops trailing keys (`c hide code`, `Tab`, `y`, `Y`) and status messages wrap. Assert on the start of a message (`press Ctrl-Q`) and on panel content rather than help-bar labels.
- Problem page focus is the panel whose title is a magenta pill (`48;2;255;42;109m <Title>`). `p` and `o` move focus as a side effect, so assert focus right after the key that sets it.
- Pressing `C`, `V`, or `Z` without the stubs would open a real GUI editor on the host. Always go through `drive.py`.
