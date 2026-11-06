#!/usr/bin/env bash
# run_all_graders.sh — run G1–G5 and G7, and print a summary table
#
# Usage:
#   ./scripts/db-code-graders/run_all_graders.sh
#   ./scripts/db-code-graders/run_all_graders.sh --diff   # only diff-mode for G1/G2
#
# Exit: 0=all PASS, 1=any FAIL, 2=warnings only

set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${1:-}"

# Make all graders executable
chmod +x "$DIR"/g*.sh

echo "╔══════════════════════════════════════════════════════════╗"
echo "║         DB ↔ Code Static Grader Suite                   ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

declare -A RESULTS=()
declare -A OUTPUTS=()

run_grader() {
    local id="$1"
    local label="$2"
    local script="$3"
    shift 3
    local args=("$@")

    local out
    local exit_code
    out=$("$script" "${args[@]}" 2>&1) || exit_code=$?
    exit_code=${exit_code:-0}

    case "$exit_code" in
        0) RESULTS[$id]="PASS" ;;
        1) RESULTS[$id]="FAIL" ;;
        2) RESULTS[$id]="WARN" ;;
        *) RESULTS[$id]="ERR($exit_code)" ;;
    esac

    OUTPUTS[$id]="$out"
}

# ── Run each grader ────────────────────────────────────────────────────────────

if [ "$MODE" = "--diff" ]; then
    run_grader G1 "INSERT completeness" python3 "$DIR/g1_insert_completeness.py" --diff
    run_grader G2 "Migration DDL impact" "$DIR/g2_migration_impact.sh"
else
    run_grader G1 "INSERT completeness" python3 "$DIR/g1_insert_completeness.py" --full
    run_grader G2 "Migration DDL impact" "$DIR/g2_migration_impact.sh"
fi

run_grader G3 "Untracked migrations" "$DIR/g3_untracked_migrations.sh"
run_grader G4 "Raw sqlx::query()" "$DIR/g4_raw_queries.sh"
run_grader G5 "Bad SQL patterns" "$DIR/g5_sql_patterns.sh"
run_grader G7 "Declared targets tracked" "$DIR/g7_declared_targets_tracked.sh"

# ── Summary table ─────────────────────────────────────────────────────────────
echo "┌──────┬──────────────────────────────┬────────┐"
echo "│  ID  │ Grader                       │ Result │"
echo "├──────┼──────────────────────────────┼────────┤"

overall=0
for id in G1 G2 G3 G4 G5 G7; do
    result="${RESULTS[$id]:-ERR}"
    case $result in
        PASS) icon="✓" ;;
        WARN) icon="~" ;;
        FAIL) icon="✗"; overall=1 ;;
        SKIP) icon="─" ;;
        *)    icon="?"; overall=1 ;;
    esac
    case $id in
        G1) label="INSERT completeness" ;;
        G2) label="Migration DDL impact" ;;
        G3) label="Untracked migrations" ;;
        G4) label="Raw sqlx::query()" ;;
        G5) label="Bad SQL patterns" ;;
        G7) label="Declared targets tracked" ;;
        *)  label="Unknown" ;;
    esac
    printf "│  %-4s│ %-28s │ %s %-5s│\n" "$id" "$label" "$icon" "$result"
done
echo "└──────┴──────────────────────────────┴────────┘"
echo ""

# ── Detail output for non-PASS graders ────────────────────────────────────────
for id in G1 G2 G3 G4 G5 G7; do
    result="${RESULTS[$id]:-ERR}"
    if [ "$result" != "PASS" ] && [ "$result" != "SKIP" ]; then
        echo "━━━ $id detail ━━━"
        echo "${OUTPUTS[$id]}"
        echo ""
    fi
done

# ── Interpretation ─────────────────────────────────────────────────────────────
echo "━━━ What each grader catches ━━━"
echo ""
echo "G1 — INSERT completeness"
echo "   Bug class: 20260526 (session_id NOT NULL added, INSERT site missed)"
echo "   How: parses migrations for NOT NULL additions, checks all INSERT sites in *.rs"
echo "   Cost: free, <2s"
echo ""
echo "G7 — Declared targets tracked"
echo "   Bug class: 20260829 (Cargo.toml declared five [[test]] targets whose files"
echo "              were never git-added; cargo test, clippy --all-targets and nextest"
echo "              all failed from a clean checkout while the author's tree worked)"
echo "   How: resolves every declared target path, every #[path] test module and every"
echo "        nextest binary() filter against the git INDEX, not the worktree"
echo "   Cost: free, <1s"
echo ""
echo "G2 — Migration DDL impact"
echo "   Bug class: DROP COLUMN, RENAME COLUMN, DROP TABLE — missed Rust references"
echo "   How: parses DDL verbs in migrations, greps *.rs for affected identifiers"
echo "   Cost: free, <5s"
echo ""
echo "G3 — Untracked migrations"
echo "   Bug class: migration applied to DB but not committed; another dev's DB is behind"
echo "   How: git ls-files --others on migrations/*.sql"
echo "   Cost: free, instant"
echo ""
echo "G4 — Raw sqlx::query()"
echo "   Bug class: bind count mismatch invisible at compile time; caught only at runtime"
echo "   How: greps for sqlx::query( (no !) in persistence layer"
echo "   Permanent fix: migrate to sqlx::query!() + sqlx prepare + SQLX_OFFLINE=true in CI"
echo "   Cost: free, <2s"
echo ""
echo "G5 — Bad SQL patterns"
echo "   Bug class: O(N²) queries, row-by-row INSERTs, unscoped DELETEs (H1-H6, M1-M8)"
echo "   How: grep + awk heuristics; not exhaustive, use for triage"
echo "   Cost: free, <3s"
echo ""
echo "━━━ What these graders do NOT catch ━━━"
echo ""
echo "  - Schema/type drift (wrong Rust type for a column) → sqlx::query!() fixes this"
echo "  - Logic errors in CTE WHERE clauses → review + test"
echo "  - Partition key errors on INSERT → sqlx::query!() + integration tests"
echo "  - Migration ordering bugs (like F7) → migration smoke tests in tests/schema/"
echo "  - Data loss from correct but semantically wrong SQL → test with real data shapes"
echo ""

exit $overall
