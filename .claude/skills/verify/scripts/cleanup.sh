#!/bin/bash
# Kills tmux servers and removes temp HOMEs left by interrupted or --keep runs.
# Only touches names drive.py creates (grind-verify-*); artifacts in target/verify stay.
for sock in "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)"/grind-verify-*; do
  [ -S "$sock" ] || continue
  tmux -L "$(basename "$sock")" kill-server 2>/dev/null
  rm -f "$sock"
  echo "killed $(basename "$sock")"
done
for home in "${TMPDIR:-/tmp}"/grind-verify-home-*; do
  [ -d "$home" ] || continue
  rm -rf "$home"
  echo "removed $home"
done
