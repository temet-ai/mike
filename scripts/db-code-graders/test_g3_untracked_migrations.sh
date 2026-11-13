#!/usr/bin/env bash
# test_g3_untracked_migrations.sh — regression test for mike#522: under
# --commit-scope, G3 must not make seats that each own a migration commit one
# after another. A commit that carries a migration blocks only on untracked
# migrations with a LOWER timestamp and warns on later ones.
#
# Usage: scripts/db-code-graders/test_g3_untracked_migrations.sh
# Exit: 0 pass, 1 fail.
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git -C "$script_dir" rev-parse --show-toplevel)"
MIG_DIR="crates/mike-base/src/store/persistence/migrations"

pass=0 fail=0
ok()  { pass=$((pass+1)); printf '  PASS  %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n     -> %s\n' "$1" "$2"; printf '%s\n' "$3" | sed 's/^/       | /'; }

work_dirs=()
cleanup() { local d; for d in ${work_dirs[@]+"${work_dirs[@]}"}; do rm -rf "$d"; done; }
trap cleanup EXIT

new_repo() {
    local d; d="$(mktemp -d)"
    work_dirs+=("$d")
    mkdir -p "$d/scripts/lib" "$d/scripts/db-code-graders" "$d/$MIG_DIR"
    cp "$REPO/scripts/lib/git-change-scope.sh" "$d/scripts/lib/"
    cp "$REPO/scripts/db-code-graders/g3_untracked_migrations.sh" "$d/scripts/db-code-graders/"
    git -C "$d" init -q .
    git -C "$d" config user.email g3@test
    git -C "$d" config user.name g3
    git -C "$d" add -A >/dev/null
    git -C "$d" commit -qm base
    printf '%s\n' "$d"
}

G3() { (cd "$1" && shift && GCS_STAGE_PATHS="${STAGE:-}" MIKE_INSPECT_URL=http://127.0.0.1:9 bash scripts/db-code-graders/g3_untracked_migrations.sh "$@" 2>&1); }

mig() { printf 'ALTER TABLE t ADD COLUMN %s text;\n' "$2" >"$1/$MIG_DIR/$3"; }

echo "mike#522 — G3 with parallel migrations"

A=20261007100000_a.sql
B=20261007100100_b.sql

for scope in cached worktree; do
    # Two seats, two migrations. The commit of A sees B untracked and vice versa.
    d="$(new_repo)"
    mig "$d" a "$A"; mig "$d" b "$B"
    git -C "$d" add "$MIG_DIR/$A" >/dev/null
    STAGE="$MIG_DIR/$A"
    out="$(G3 "$d" --commit-scope "$scope")"; rc=$?
    if [[ $rc -eq 0 && "$out" == *"WARN"* && "$out" == *"$B"* ]]; then ok "[$scope] older migration commits while a later one is untracked (warn only)"
    else bad "[$scope] older migration commits while a later one is untracked" "exit=$rc" "$out"; fi
    git -C "$d" commit -qm a -- "$MIG_DIR/$A"

    git -C "$d" add "$MIG_DIR/$B" >/dev/null
    STAGE="$MIG_DIR/$B"
    out="$(G3 "$d" --commit-scope "$scope")"; rc=$?
    if [[ $rc -eq 0 ]]; then ok "[$scope] the later migration commits after the older landed"
    else bad "[$scope] the later migration commits after the older landed" "exit=$rc" "$out"; fi

    # Reverse order: the later migration first, the older one untracked → blocks.
    d="$(new_repo)"
    mig "$d" a "$A"; mig "$d" b "$B"
    git -C "$d" add "$MIG_DIR/$B" >/dev/null
    STAGE="$MIG_DIR/$B"
    out="$(G3 "$d" --commit-scope "$scope")"; rc=$?
    if [[ $rc -eq 1 && "$out" == *"FAIL"* && "$out" == *"$A"* ]]; then ok "[$scope] an older untracked migration still blocks a later one"
    else bad "[$scope] an older untracked migration still blocks a later one" "exit=$rc" "$out"; fi
done
STAGE=""

# Both orders are possible for the two commits when the seats own different ones:
d="$(new_repo)"
mig "$d" a "$A"; mig "$d" b "$B"
git -C "$d" add "$MIG_DIR/$A" "$MIG_DIR/$B" >/dev/null
git -C "$d" commit -qm both -- "$MIG_DIR/$A" "$MIG_DIR/$B"
out="$(G3 "$d" --commit-scope cached)"; rc=$?
if [[ $rc -eq 0 && "$out" == *"PASS"* ]]; then ok "nothing untracked passes"
else bad "nothing untracked passes" "exit=$rc" "$out"; fi

# Audit mode (no --commit-scope) is unchanged: any untracked migration fails.
d="$(new_repo)"
mig "$d" b "$B"
out="$(G3 "$d")"; rc=$?
if [[ $rc -eq 1 && "$out" == *"FAIL"* ]]; then ok "audit mode still blocks on any untracked migration"
else bad "audit mode still blocks on any untracked migration" "exit=$rc" "$out"; fi

# Incident guard: a commit with query code and no migration of its own still
# blocks on every untracked migration.
d="$(new_repo)"
mig "$d" b "$B"
mkdir -p "$d/src"
printf 'fn q() { sqlx::query!("SELECT foo FROM t"); }\n' >"$d/src/store.rs"
git -C "$d" add src/store.rs >/dev/null
out="$(G3 "$d" --commit-scope cached)"; rc=$?
if [[ $rc -eq 1 && "$out" == *"SQL-shaped"* ]]; then ok "query code with no migration of its own still blocks"
else bad "query code with no migration of its own still blocks" "exit=$rc" "$out"; fi

echo "────────────────────────────────────────"
printf 'passed=%d failed=%d\n' "$pass" "$fail"
[[ "$fail" -eq 0 ]] || exit 1
exit 0
