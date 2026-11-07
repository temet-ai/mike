#!/usr/bin/env bash
# Commit-time Rust lint on the staged files only, so a peer's unfinished file in
# a shared checkout never blocks a commit. Formats each file and stages the
# result, and refuses a file over 500 lines. A file with unstaged changes is
# refused rather than formatted, so those changes never join the commit.
# Pass paths to format those without staging anything.
set -uo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root" || exit 2

max_lines=500
auto_add=0
if [ "$#" -gt 0 ]; then
  files=$(printf '%s\n' "$@")
else
  auto_add=1
  files=$(git diff --cached --name-only --diff-filter=ACMR -- '*.rs')
fi
[ -n "$files" ] || exit 0

rustfmt_bin=$(rustup which rustfmt 2>/dev/null || command -v rustfmt || true)
status=0
args=()
while IFS= read -r file; do
  [ -f "$file" ] || continue
  args+=("$file")
  lines=$(awk 'END { print NR }' "$file")
  if [ "$lines" -gt "$max_lines" ]; then
    echo "$file: $lines lines, over the $max_lines limit"
    status=1
  fi
done <<<"$files"

if [ -n "$rustfmt_bin" ] && [ "${#args[@]}" -gt 0 ]; then
  formatted=$(mktemp "${TMPDIR:-/tmp}/rust-staged-lint.XXXXXX")
  trap 'rm -f "$formatted"' EXIT
  for file in "${args[@]}"; do
    if ! "$rustfmt_bin" --edition 2024 <"$file" >"$formatted" 2>/dev/null; then
      echo "$file: rustfmt could not parse it"
      status=1
      continue
    fi
    cmp -s "$file" "$formatted" && continue
    if [ "$auto_add" = 1 ] && ! git diff --quiet -- "$file"; then
      echo "$file: not formatted, and it has unstaged changes; run make fmt"
      status=1
      continue
    fi
    cat "$formatted" >"$file"
    echo "$file: formatted"
    if [ "$auto_add" = 1 ]; then
      git add -- "$file"
      printf '%s\n' "$file" >>"$(git rev-parse --git-dir)/rust-staged-formatted"
    fi
  done
fi
exit "$status"
