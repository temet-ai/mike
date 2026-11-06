#!/usr/bin/env bash
# sql-set-based.sh — executable form of the .claude/rules/rust-database-reliability.md self-check.
# Flags correlated scalar subqueries in the diff (EXISTS / NOT EXISTS excluded).
# Prints offending lines, or "CLEAN". Exit 0 = clean, 1 = hits found.
# Usage: sql-set-based.sh [git-diff-range]   (default: working tree vs HEAD)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
hits=$(git diff ${1:-} -- '*.rs' '*.sql' \
  | grep -nE '\(SELECT' \
  | grep -viE 'EXISTS *\(SELECT|NOT EXISTS *\(SELECT' \
  || true)
if [ -n "$hits" ]; then printf '%s\n' "$hits"; exit 1; fi
echo CLEAN
