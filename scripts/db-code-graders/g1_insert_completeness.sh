#!/usr/bin/env bash
# G1 — INSERT completeness grader
#
# Catches the 20260526 bug class: a migration adds a NOT NULL column to a
# table, and one or more INSERT sites in Rust code don't set it.
#
# Usage:
#   ./g1_insert_completeness.sh [--diff]   # check git working-tree diff only
#   ./g1_insert_completeness.sh [--full]   # check all *.rs files (default)
#   ./g1_insert_completeness.sh [--migration <file.sql>]  # check specific migration
#
# Exit codes: 0 = PASS, 1 = FAIL, 2 = SKIP (no relevant migrations found)

set -euo pipefail

MODE="${1:---full}"
MIGRATION_ARG="${2:-}"
CRATE_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"
MIGRATIONS_DIR="$CRATE_ROOT/crates/mike/src/store/persistence/migrations"
RS_GLOB="$CRATE_ROOT/crates/**/*.rs"

# ──────────────────────────────────────────────────────────────────────────────
# Step 1: Find migrations that add NOT NULL columns without a DEFAULT.
#         These are the dangerous ones — they require every INSERT to supply
#         the column value.
# ──────────────────────────────────────────────────────────────────────────────

find_notnull_additions() {
    local migration_file="$1"
    # Pattern 1: ADD COLUMN col_name TYPE ... NOT NULL  (without DEFAULT)
    # e.g.  ADD COLUMN session_id TEXT NOT NULL
    grep -iE '^\s*(ALTER TABLE.*ADD COLUMN|ADD COLUMN)[^;]+NOT NULL' "$migration_file" \
        | grep -ivE 'DEFAULT\s+\S' \
        | sed -n "s/.*ADD COLUMN\s\+\(IF NOT EXISTS\s\+\)\?\([a-z_][a-z0-9_]*\)\s.*/\2/Ip"

    # Pattern 2: Two-step migration — ADD COLUMN ... NULL (for backfill), then SET NOT NULL
    # e.g.  ALTER TABLE ... ALTER COLUMN session_id SET NOT NULL
    # This is the pattern used by migration 20260526000002.
    grep -iE '^\s*ALTER\s+COLUMN\s+[a-z_]+\s+SET\s+NOT\s+NULL' "$migration_file" \
        | sed -n "s/.*ALTER\s\+COLUMN\s\+\([a-z_][a-z0-9_]*\)\s\+SET.*/\1/Ip"
}

find_table_from_migration() {
    local migration_file="$1"
    # Extract the table name from ALTER TABLE clauses
    grep -iE '^\s*ALTER TABLE\s+[a-z_.]+' "$migration_file" \
        | head -1 \
        | sed -n 's/.*ALTER TABLE\s\+\(mike\.[a-z_]*\).*/\1/Ip'
}

# ──────────────────────────────────────────────────────────────────────────────
# Step 2: For a given table + column, find all INSERT sites in Rust code and
#         check that each one sets the column.
# ──────────────────────────────────────────────────────────────────────────────

check_inserts_for_column() {
    local table="$1"     # e.g. mike.response_stream_chunks
    local column="$2"    # e.g. session_id
    local rs_files="$3"  # newline-separated list of .rs files to search

    local short_table
    short_table=$(echo "$table" | sed 's/mike\.//')

    local pass=0
    local fail=0
    local missing_sites=()

    while IFS= read -r rs_file; do
        [ -f "$rs_file" ] || continue

        # Find line numbers of INSERT INTO <table> or <short_table>
        local insert_lines
        insert_lines=$(grep -inE "INSERT INTO (mike\.)?${short_table}[^a-z_]" "$rs_file" \
            | cut -d: -f1 || true)

        for lineno in $insert_lines; do
            # Extract a window: the INSERT line + next 20 lines (covers column list + VALUES)
            local window
            window=$(awk "NR>=$lineno && NR<=$((lineno+20))" "$rs_file")

            if echo "$window" | grep -iqE "\b${column}\b"; then
                pass=$((pass + 1))
            else
                fail=$((fail + 1))
                missing_sites+=("  $rs_file:$lineno — INSERT missing '$column'")
            fi
        done
    done <<< "$rs_files"

    echo "  Table: $table | Column: $column"
    echo "  INSERT sites checked: $((pass + fail)) | passing: $pass | failing: $fail"
    if [ ${#missing_sites[@]} -gt 0 ]; then
        for site in "${missing_sites[@]}"; do
            echo "$site"
        done
        return 1
    fi
    return 0
}

# ──────────────────────────────────────────────────────────────────────────────
# Main
# ──────────────────────────────────────────────────────────────────────────────

ALL_RS_FILES=$(find "$CRATE_ROOT/crates" -name "*.rs" \
    ! -path "*/target/*" ! -path "*/tests/*" 2>/dev/null | sort)

if [[ "$MODE" == "--diff" ]]; then
    CHANGED_RS=$(git diff --name-only HEAD -- '*.rs' 2>/dev/null \
        | sed "s|^|$CRATE_ROOT/|" | tr '\n' '\n')
    RS_TO_SCAN="$CHANGED_RS"
else
    RS_TO_SCAN="$ALL_RS_FILES"
fi

# Which migrations to scan
if [[ "$MODE" == "--migration" ]] && [[ -n "$MIGRATION_ARG" ]]; then
    MIGRATION_FILES="$MIGRATION_ARG"
elif [[ "$MODE" == "--diff" ]]; then
    MIGRATION_FILES=$(git diff --name-only HEAD -- '*.sql' 2>/dev/null \
        | sed "s|^|$CRATE_ROOT/|")
else
    # All migrations
    MIGRATION_FILES=$(find "$MIGRATIONS_DIR" -name "*.sql" 2>/dev/null | sort)
fi

overall_fail=0
checked=0

echo "=== G1: INSERT completeness grader ==="
echo "Mode: $MODE | RS files: $(echo "$RS_TO_SCAN" | grep -c '\.rs' || echo 0)"
echo ""

while IFS= read -r migration_file; do
    [ -f "$migration_file" ] || continue

    table=$(find_table_from_migration "$migration_file")
    [ -z "$table" ] && continue

    columns=$(find_notnull_additions "$migration_file")
    [ -z "$columns" ] && continue

    echo "Migration: $(basename "$migration_file")"
    echo "  Table: $table"

    while IFS= read -r column; do
        [ -z "$column" ] && continue
        checked=$((checked + 1))
        if ! check_inserts_for_column "$table" "$column" "$RS_TO_SCAN"; then
            overall_fail=$((overall_fail + 1))
        fi
    done <<< "$columns"
    echo ""
done <<< "$MIGRATION_FILES"

echo "────────────────────────────────────────"
if [ "$checked" -eq 0 ]; then
    echo "SKIP: no NOT NULL column additions found in scanned migrations"
    exit 2
elif [ "$overall_fail" -eq 0 ]; then
    echo "PASS: all $checked NOT NULL column addition(s) have complete INSERT coverage"
    exit 0
else
    echo "FAIL: $overall_fail/$checked column(s) have incomplete INSERT coverage"
    exit 1
fi
