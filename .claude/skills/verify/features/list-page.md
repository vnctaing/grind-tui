# List page

The launch screen lists all 167 Grind 75 and NeetCode 150 problems. The user filters by list, searches by title or id, marks problems solved, and opens one.

## Sub-features

- `list-filter` cycles All, Grind 75, NeetCode 150 with `t`.
- `list-search` filters by title substring or id with `/`.
- `list-open` opens the selected problem with `Enter`; `b` returns.
- `list-solved` toggles solved with `x`; the header counter and a check mark update, and quit persists it.
- `list-cli` starts filtered with `--list grind75|neetcode150` and `--difficulty easy|medium|hard`.

## How to get to it (user POV)

- Run `grind`. The list is the first screen.
- `j`/`k` or arrows move, `q` quits and writes `state.json`.

## Driving it with drive.py

Preconditions:

- Fresh HOME, size 140x40 or 80x24.

- **Filter.** `key t` three times. Counter reads `75 problems`, `150 problems`, then `167 problems`; help bar shows `t Grind 75`, `t NeetCode 150`.
- **Search.** `key /` then `text parenth`. Rows `Valid Parentheses` and `Generate Parentheses` show, `Two Sum` does not.
- **Open.** `key Esc` then `key Enter`. Editor bar shows `valid-parentheses.js`; `key b` returns.
- **Solved.** `key x`. Header reads `1/167` and the row has a check mark.
- **Persist.** `key q`, `expect-exit 0`, `expect-file .grind-tui/state.json ~ "last_problem_id": 20`.
- **CLI filter.** `launch --list grind75 --difficulty easy` then `expect` an Easy-only count. Not in the canonical scenario.

## Gotchas

- `Esc` closes the search box but keeps the query; `/` again clears it.
- `state.json` is written only on quit, so assert it after `expect-exit`.
