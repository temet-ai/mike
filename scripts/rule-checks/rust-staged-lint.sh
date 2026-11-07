#!/usr/bin/env bash
# Commit-time Rust lint: formatting and the 500-line limit, on the staged files
# only, so a peer's unfinished file in a shared checkout never blocks a commit.
# Pass paths to check those instead of the staged files.
set -uo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root" || exit 2

max_lines=500
if [ "$#" -gt 0 ]; then
  files=$(printf '%s\n' "$@")
else
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
  output=$("$rustfmt_bin" --edition 2024 --check "${args[@]}" 2>&1)
  for file in "${args[@]}"; do
    if printf '%s\n' "$output" | grep -qF "Diff in $root/${file#"$root"/}"; then
      echo "$file: not formatted; run make fmt"
      status=1
    fi
  done
fi
exit "$status"
