#!/usr/bin/env bash
# .git/hooks/pre-commit — DB↔Code grader gate
#
# Runs before every `git commit`. Blocks the commit if any grader fails.
# This hook and the commit-msg hook (commit-msg-hook.sh, which runs G6) are the
# only commit-time enforcement; they cover every commit, whoever makes it.
# G2 and G5 do not run here; run_all_graders.sh runs them.
#
# This file is NOT committed to the repo (.git/hooks is ignored).
# It is installed by: scripts/db-code-graders/install-hooks.sh
# Re-install after a fresh clone: run that script once.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
GRADERS="$REPO_ROOT/scripts/db-code-graders"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT

# Private words (word list outside the repo) never enter a commit.
"$REPO_ROOT/scripts/private-words-check" --staged || exit 1

if command -v gitleaks >/dev/null 2>&1; then
    gitleaks git --pre-commit --staged --no-banner --redact "$REPO_ROOT" >/dev/null 2>&1 || {
        echo "Commit blocked: gitleaks found a secret; run: gitleaks git --pre-commit --staged --redact ." >&2
        exit 1
    }
fi
python3 "$REPO_ROOT/scripts/rule-checks/frontend-staged-lint.py" || exit 1

# Docs/infra-only commits can't affect DB↔code integrity, so skip the DB graders
# when nothing code- or SQL-shaped is staged. G4 (raw sqlx in persistence)
# inspects the working TREE, not the staged diff, so without this guard an
# unrelated .rs edit sitting in the tree would block a .md-only commit. G3 now
# scopes itself (--commit-scope below), which is what a .rs-only commit needs:
# it reaches G3 past this guard and must not be frozen by a migration it does
# not carry (mike#163).
if ! git diff --cached --name-only | grep -qE '\.(rs|sql)$'; then
    echo "✓ No .rs/.sql staged — DB↔Code graders not applicable (docs/infra-only commit)"
    exit 0
fi

failures=()
messages=()

if ! g8_out=$(bash "$REPO_ROOT/scripts/rule-checks/rust-no-comments.sh" --cached 2>&1); then
    failures+=("G8")
    messages+=("G8 — comments added to Rust:"$'\n'"$(echo "$g8_out" | sed -n '1,10p')")
fi

if ! g9_out=$(bash "$REPO_ROOT/scripts/rule-checks/rust-staged-lint.sh" 2>&1); then
    failures+=("G9")
    messages+=("G9 — staged Rust fails lint:"$'\n'"$(echo "$g9_out" | sed -n '1,10p')")
fi

echo "🔍 Running DB↔Code graders before commit..."

# ── G3: Untracked migration files ─────────────────────────────────────────────
# --commit-scope cached: this hook runs after staging, so the index IS the
# commit. Blocks only when the commit itself carries SQL; otherwise the finding
# is printed and the commit proceeds, so a peer's migration does not freeze it.
if ! bash "$GRADERS/g3_untracked_migrations.sh" --commit-scope cached > "$out/g3.txt" 2>&1; then
    failures+=("G3")
    untracked=$(grep -E '^\s+!!' "$out/g3.txt" | sed 's/.*!!\s*//' | xargs -n1 basename 2>/dev/null | sed -n '1,3p' | tr '\n' ', ' | sed 's/,$//')
    messages+=("G3 — untracked migrations: ${untracked}")
elif grep -qE '^WARN — (Untracked|Modified)' "$out/g3.txt"; then
    echo ""
    echo "⚠ G3 raised a finding this commit does not have to fix:"
    sed 's/^/  /' "$out/g3.txt"
    echo "  It belongs to whoever wrote the file — tell them, don't move it (mike#163)."
    echo ""
fi

# ── G7: Declared build targets are tracked ────────────────────────────────────
# Same scoping rule as G3, same reason: block only a commit that itself touches
# the declaration surface (a Cargo.toml, the nextest config, a tests/ file), so
# one session's in-flight untracked file cannot freeze everyone else's commits.
# This is the gate that would have stopped 2026-08-29, where a Cargo.toml naming
# five [[test]] entry points landed without the files, and cargo test, clippy
# --all-targets and nextest all broke from a clean checkout.
if ! bash "$GRADERS/g7_declared_targets_tracked.sh" --commit-scope cached > "$out/g7.txt" 2>&1; then
    failures+=("G7")
    g7_hits=$(grep -E '^\s+!!' "$out/g7.txt" | sed -n '1,3p' | sed 's/^[[:space:]]*//')
    messages+=("G7 — this commit declares a build target whose file is not staged. git add it, or drop the declaration:"$'\n'"$g7_hits")
elif grep -qE '^WARN — ' "$out/g7.txt"; then
    echo ""
    echo "⚠ G7 raised a finding this commit does not have to fix:"
    sed 's/^/  /' "$out/g7.txt"
    echo ""
fi

# ── G1: INSERT completeness (diff mode — only this commit's migrations) ────────
exit_code=0
python3 "$GRADERS/g1_insert_completeness.py" --diff > "$out/g1.txt" 2>&1 || exit_code=$?
if [ "$exit_code" -ne 0 ]; then
    if [ "$exit_code" -eq 1 ]; then
        failures+=("G1")
        fail_lines=$(grep 'FAIL:' "$out/g1.txt" | sed -n '1,3p' | sed 's/^[[:space:]]*//')
        messages+=("G1 — INSERT sites missing new NOT NULL column:"$'\n'"$fail_lines")
    fi
    # exit 2 = SKIP (no relevant migrations) — not a failure
fi

# ── G4: Raw sqlx::query() in the persistence layer ────────────────────────────
# Delegates to g4_raw_queries.sh (single source of truth) rather than a naive
# diff-grep, so it shares that script's logic: it excludes the trailing
# `#[cfg(test)] mod tests` block (raw queries in test seed data are acceptable
# per the severity legend) and honours explicit `// grader-allow: <reason>`
# markers (documented exceptions — e.g. pgvector `$N::vector` casts that
# sqlx::query! cannot type-check). --commit-scope cached: same fix as G3/G7
# (mike#163) — the grader inspects the whole tree, so without this a raw
# query already on main before this commit would freeze every unrelated .rs
# commit forever; scoped, it blocks only a HIGH site this commit's own diff
# adds, and reports a pre-existing one as a non-blocking advisory instead.
# Exit 1 = a HIGH site added by this commit (block); exit 2 = MEDIUM-only
# warnings (non-blocking); exit 0 = clean or pre-existing-only.
bash "$GRADERS/g4_raw_queries.sh" --commit-scope cached > "$out/g4.txt" 2>&1 && g4_exit=0 || g4_exit=$?
if [ "${g4_exit:-0}" -eq 1 ]; then
    failures+=("G4")
    g4_hits=$(grep -E '^\s+L[0-9]+:' "$out/g4.txt" | sed -n '1,3p' | sed 's/^[[:space:]]*//')
    messages+=("G4 — raw sqlx::query() in persistence (migrate to sqlx::query!() or add a documented // grader-allow: marker):"$'\n'"$g4_hits")
elif grep -qE '^PASS \(advisory\)' "$out/g4.txt"; then
    echo ""
    echo "⚠ G4 raised a finding this commit does not have to fix:"
    grep -E '^(PASS \(advisory\)|  Not this commit)' "$out/g4.txt" | sed 's/^/  /'
    echo ""
fi

# ── Result ────────────────────────────────────────────────────────────────────
if [ ${#failures[@]} -eq 0 ]; then
    echo "✓ All DB↔Code graders passed"
    exit 0
fi

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  COMMIT BLOCKED — DB↔Code grader(s) failed              ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""
for msg in "${messages[@]}"; do
    echo "• $msg"
    echo ""
done
echo "Run: scripts/db-code-graders/run_all_graders.sh"
echo ""
exit 1
