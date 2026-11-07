#!/usr/bin/env bash
# A `git commit -- <paths>` runs its hooks on a temporary index, so a file the
# pre-commit lint formatted stays old in the real index. This post-commit hook
# brings those files back into line, only where the working copy matches HEAD.
set -uo pipefail
list="$(git rev-parse --git-dir)/rust-staged-formatted"
[ -f "$list" ] || exit 0
while IFS= read -r file; do
  if git diff --quiet HEAD -- "$file"; then
    git reset -q -- "$file"
  fi
done <"$list"
rm -f "$list"
exit 0
