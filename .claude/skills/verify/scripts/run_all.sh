#!/bin/bash
# Runs every scenario at 140x40 and 80x24; prints one line per run and writes
# target/verify/SUMMARY.txt. Exit status is non-zero if any run failed.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
sizes="${SIZES:-140x40 80x24}"
summary="$repo/target/verify/SUMMARY.txt"
mkdir -p "$repo/target/verify"
: > "$summary"
status=0
for scn in "$here"/../scenarios/${1:-*}.scn; do
  for size in $sizes; do
    if python3 -I "$here/drive.py" "$scn" --size "$size" > /dev/null 2>&1; then r=PASS; else r=FAIL; status=1; fi
    line="$r $(basename "$scn" .scn) $size -> $repo/target/verify/$(basename "$scn" .scn)-$size"
    echo "$line" | tee -a "$summary"
  done
done
exit $status
