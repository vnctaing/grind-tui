#!/bin/bash
# Read-only: are the tools present, is the release binary current, are any runs stranded?
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
ok=0
for tool in tmux freeze sandbox-exec cargo python3; do
  if command -v "$tool" >/dev/null; then echo "ok    $tool $(command -v "$tool")"; else echo "MISS  $tool"; ok=1; fi
done
bin="$repo/target/release/grind"
if [ -x "$bin" ]; then
  newer=$(find "$repo/src" "$repo/data" "$repo/Cargo.toml" -newer "$bin" -type f | head -3)
  if [ -n "$newer" ]; then echo "STALE $bin (older than: $newer); drive.py rebuilds it"; else echo "ok    $bin is current with src/"; fi
else
  echo "NOBIN $bin; drive.py builds it"
fi
stray=$(ls "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)"/ 2>/dev/null | grep '^grind-verify-')
homes=$(ls -d "${TMPDIR:-/tmp}"/grind-verify-home-* 2>/dev/null)
if [ -n "$stray$homes" ]; then echo "STRAY $stray $homes; run cleanup.sh"; else echo "ok    no stranded tmux servers or temp HOMEs"; fi
exit $ok
