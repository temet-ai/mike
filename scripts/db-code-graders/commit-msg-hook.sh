#!/usr/bin/env bash
# .git/hooks/commit-msg — refuses a commit message that carries a private word.
# Installed by scripts/db-code-graders/install-hooks.sh.
# $1 = path to the file holding the proposed commit message.
set -uo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
"$REPO_ROOT/scripts/private-words-check" --message "$1" || exit 1
