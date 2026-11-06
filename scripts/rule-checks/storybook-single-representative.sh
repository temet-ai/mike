#!/usr/bin/env bash
# storybook-single-representative.sh — executable form of the
# .claude/rules/frontend-development.md self-check.
# Flags touched *.stories.svelte files carrying more than one <Story>
# (the __foundations__ galleries are exempt).
# Prints REVIEW(...) lines, or "CLEAN". Exit 0 = clean, 1 = needs operator sign-off.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
found=0
while read -r f; do
  [ -f "$f" ] || continue
  case "$f" in */__foundations__/*) continue;; esac
  n=$(grep -c '<Story' "$f")
  if [ "$n" -gt 1 ]; then echo "REVIEW($n stories): $f"; found=1; fi
done < <(git diff --name-only -- '*.stories.svelte')
[ "$found" -eq 0 ] && echo CLEAN
exit "$found"
