#!/usr/bin/env bash
# .git/hooks/commit-msg — G6 perf-evidence gate
#
# Runs on EVERY commit (editor commits and `git commit -m` alike, so it covers
# both human and Claude Code commits — a pre-commit hook cannot see the message,
# which is why this is a commit-msg hook). Blocks the commit when the staged diff
# touches SQL but the message carries no `Perf-Verdict:` (or records a
# regression). Enforces .claude/rules/rust-database-reliability.md.
#
# $1 = path to the file holding the proposed commit message.
#
# This file is NOT committed (.git/hooks is ignored). Installed by
# scripts/db-code-graders/install-hooks.sh — re-run after a fresh clone.
set -uo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
"$REPO_ROOT/scripts/private-words-check" --message "$1" || exit 1
G6="$REPO_ROOT/scripts/db-code-graders/g6_perf_evidence.sh"
[[ -x "$G6" ]] || exit 0   # fail-open: missing grader never bricks committing

if bash "$G6" --message-file "$1"; then
    exit 0
fi

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  COMMIT BLOCKED — G6 perf-evidence gate                   ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo "See .claude/rules/rust-database-reliability.md"
exit 1
