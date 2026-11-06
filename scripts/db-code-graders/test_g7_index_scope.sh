#!/usr/bin/env bash
# test_g7_index_scope.sh — regression test for the G7 disk-vs-index bug
# (fleet-judgement-at-work, 2026-09-28): in --commit-scope cached mode,
# g7_declared_targets_tracked.sh scanned a declaration file (Cargo.toml, a
# tests/*.rs entry point, .config/nextest.toml) from the WORKING TREE rather
# than the git INDEX. In a shared checkout with several sessions' uncommitted
# mod/path lines sitting in the same worktree file at once, that meant a
# commit whose own staged diff never touched those lines still failed,
# because the grader was reading (and flagging) lines nobody was about to
# commit. g7_declared_targets_tracked.sh now reads every declaration file
# via `dcat` (`git show :<path>` in cached mode), which sees the index — and
# a private GIT_INDEX_FILE, so this test never touches the real repo index.
#
# Case: a tracked tests/*.rs entry point. The WORKTREE copy declares two
# `#[path = "..."]` modules, only one of which is tracked. The INDEX (what a
# `git commit` would actually ship) declares only the tracked one. Expect
# PASS: nothing about the untracked module in the working tree should be
# able to fail a commit that never staged it.
#
# Usage: scripts/db-code-graders/test_g7_index_scope.sh
# Exit: 0 pass, 1 fail.
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
g7="$repo_root/scripts/db-code-graders/g7_declared_targets_tracked.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

git init -q "$work"
git -C "$work" config user.email test@example.com
git -C "$work" config user.name test

mkdir -p "$work/crates/mike/tests"
cat > "$work/crates/mike/Cargo.toml" <<'EOF'
[package]
name = "mike"
version = "0.1.0"

[[test]]
name = "integration"
path = "tests/integration.rs"
EOF
mkdir -p "$work/crates/mike/tests/tracked_mod"
cat > "$work/crates/mike/tests/tracked_mod/mod.rs" <<'EOF'
EOF
cat > "$work/crates/mike/tests/integration.rs" <<'EOF'
#[path = "tracked_mod/mod.rs"]
mod tracked_mod;
EOF

git -C "$work" add -A
git -C "$work" commit -q -m "baseline: one tracked mod line, everything committed"

# The worktree copy of the tracked entry point gains a second, untracked mod
# line (another session's in-flight edit) but is never staged — this is the
# exact shape that broke: a live commit touching some OTHER tests/ file used
# to fail on this unstaged line because the grader read it off disk anyway.
cat >> "$work/crates/mike/tests/integration.rs" <<'EOF'
#[path = "untracked_mod/mod.rs"]
mod untracked_mod;
EOF

# A second, genuinely tracked-and-staged test file: this is what should
# actually decide blocking scope (a tests/ file IS part of this commit).
mkdir -p "$work/crates/mike/tests/other"
echo "pub fn marker() {}" > "$work/crates/mike/tests/other/mod.rs"
git -C "$work" add "crates/mike/tests/other/mod.rs"

echo "=== test_g7_index_scope ==="
fail=0

out="$(cd "$work" && bash "$g7" --commit-scope cached 2>&1)"
rc=$?
if [[ "$rc" -eq 0 ]] && ! grep -q 'untracked_mod' <<<"$out"; then
    echo "PASS: --commit-scope cached ignores the worktree-only mod line an uncommitted, unstaged edit added"
else
    echo "FAIL: --commit-scope cached should pass (exit 0, no mention of untracked_mod) but got exit $rc:"
    printf '%s\n' "$out" | sed 's/^/  /'
    fail=1
fi

out="$(cd "$work" && bash "$g7" --commit-scope worktree 2>&1)"
if grep -q 'untracked_mod' <<<"$out"; then
    echo "PASS: --commit-scope worktree still catches the same untracked module (tree mode unchanged)"
else
    echo "FAIL: --commit-scope worktree should still flag untracked_mod but did not:"
    printf '%s\n' "$out" | sed 's/^/  /'
    fail=1
fi

if [[ "$fail" -eq 0 ]]; then
    echo "=== ALL PASS ==="
else
    echo "=== FAIL ==="
fi
exit "$fail"
