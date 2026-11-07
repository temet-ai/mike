#!/usr/bin/env bash
# install-hooks.sh — install DB↔Code grader pre-commit hook into .git/hooks/
#
# Run this once after cloning the repo:
#   bash scripts/db-code-graders/install-hooks.sh
#
# The hook lives in scripts/db-code-graders/pre-commit-hook.sh (committed).
# This script symlinks it into .git/hooks/pre-commit.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK_SOURCE="$REPO_ROOT/scripts/db-code-graders/pre-commit-hook.sh"
HOOK_TARGET="$REPO_ROOT/.git/hooks/pre-commit"

chmod +x "$HOOK_SOURCE"

if [ -e "$HOOK_TARGET" ] && [ ! -L "$HOOK_TARGET" ]; then
    echo "Warning: $HOOK_TARGET already exists and is not a symlink."
    echo "Backing up to $HOOK_TARGET.bak"
    mv "$HOOK_TARGET" "$HOOK_TARGET.bak"
fi

ln -sf "$HOOK_SOURCE" "$HOOK_TARGET"
echo "✓ Installed: $HOOK_TARGET → $HOOK_SOURCE"

# ── commit-msg hook — G6 perf-evidence gate ───────────────────────────────────
# A separate hook because pre-commit cannot see the commit message, and G6 needs
# it. Runs on every commit (editor and `git commit -m`), so it covers both human
# and Claude Code commits.
CM_SOURCE="$REPO_ROOT/scripts/db-code-graders/commit-msg-hook.sh"
CM_TARGET="$REPO_ROOT/.git/hooks/commit-msg"
chmod +x "$CM_SOURCE"
if [ -e "$CM_TARGET" ] && [ ! -L "$CM_TARGET" ]; then
    echo "Warning: $CM_TARGET already exists and is not a symlink. Backing up to $CM_TARGET.bak"
    mv "$CM_TARGET" "$CM_TARGET.bak"
fi
ln -sf "$CM_SOURCE" "$CM_TARGET"
echo "✓ Installed: $CM_TARGET → $CM_SOURCE"

PP_SOURCE="$REPO_ROOT/scripts/pre-push-check"
PP_TARGET="$REPO_ROOT/.git/hooks/pre-push"
chmod +x "$PP_SOURCE"
if [ -e "$PP_TARGET" ] && [ ! -L "$PP_TARGET" ]; then
    echo "Warning: $PP_TARGET already exists and is not a symlink. Backing up to $PP_TARGET.bak"
    mv "$PP_TARGET" "$PP_TARGET.bak"
fi
ln -sf "$PP_SOURCE" "$PP_TARGET"
echo "✓ Installed: $PP_TARGET → $PP_SOURCE"
echo ""
echo "pre-commit: G8 + G9 + G3 + G7 + G1 + G4 before every git commit."
echo "commit-msg: G6 perf-evidence gate (.claude/rules/rust-database-reliability.md)."
echo "pre-push: fmt, clippy and structural lint, secret scan, cargo-deny (scripts/pre-push-check)."
echo "G2 and G5 do not run at commit time; run scripts/db-code-graders/run_all_graders.sh for them."
