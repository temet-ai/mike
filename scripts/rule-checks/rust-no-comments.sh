#!/usr/bin/env bash
# Rejects comments added to .rs files. Prints findings or "CLEAN"; exit 1 on findings.
# Usage: rust-comment-history.sh [git-diff-args]   (default: working tree vs HEAD; --cached for the index)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
git diff --unified=0 --src-prefix=a/ --dst-prefix=b/ "$@" -- '*.rs' | python3 "$(dirname "$0")/rust-comments.py"
