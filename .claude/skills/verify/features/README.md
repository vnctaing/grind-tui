# grind-tui verification map

The maintained list of what a user can do in grind-tui and how to prove each one. Read this index, then the feature file, then run its scenario.

| Feature | File | Canonical scenario |
| --- | --- | --- |
| List page: filters, search, open, solved, quit | [list-page.md](list-page.md) | `scenarios/list.scn` |
| Problem page: panels, focus, language, sync, yank | [problem-page.md](problem-page.md) | `scenarios/problem.scn` |
| External editors: blocking, detached, blocked | [external-editors.md](external-editors.md) | `scenarios/editors.scn` |
| Embedded editor: edit, undo, save, reload, discard | [embedded-editor.md](embedded-editor.md) | `scenarios/editor.scn` |
| Diagnostics: markers, status rows, F8 | [diagnostics.md](diagnostics.md) | `scenarios/diagnostics.scn` |

## Baseline preconditions

- Fresh temp HOME with an empty `.grind-tui/` (the app writes `editors.json` on first launch).
- No network (`sandbox-exec` denies it). Nothing in the app needs it.
- Sizes 140x40 and 80x24. `run_all.sh` runs every scenario at both.
- Opening the first row of the default `All` list opens 1 Two Sum, Python, starter `class Solution: … def twoSum`.

## Driving conventions

- Every scenario starts from launch. Seed files with `write` before `launch`.
- Assert on strings only the target state can produce. Prefer status messages and counters to help-bar labels, which truncate at 80 columns.
- Every mutation gets an `expect-file` on the written file.

## Proof and skip reporting

- Report each scenario and size as PASS or FAIL with the path to its `target/verify/<scenario>-<WxH>/` directory, and name the PNGs you looked at.
- A sub-feature you could not drive is reported as not verified, with the reason. Never count it as covered by a neighbouring scenario.

## Feature entry contract

Each feature file has an H1, one paragraph, then `Sub-features`, `How to get to it (user POV)`, `Driving it with drive.py`, and `Gotchas`.
