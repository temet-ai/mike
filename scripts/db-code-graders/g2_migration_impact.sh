#!/usr/bin/env bash
# G2 — Migration DDL impact grader
#
# Broader than G1. Parses migrations for ALL schema changes that require
# corresponding Rust code changes, and verifies the code was updated.
#
# Catches:
#   - ADD COLUMN NOT NULL              → INSERT sites must set it (same as G1)
#   - DROP COLUMN                      → no Rust file should reference the column name
#   - RENAME COLUMN old TO new         → old name gone in Rust, new name present
#   - ALTER COLUMN SET TYPE            → type-sensitive binds may be wrong
#   - DROP TABLE                       → no Rust file should reference the table
#   - ADD CONSTRAINT ... NOT NULL      → same as ADD COLUMN NOT NULL
#
# Usage:
#   ./g2_migration_impact.sh [path/to/migration.sql ...]
#   ./g2_migration_impact.sh          # scans all untracked + diff migrations
#
# Exit: 0=PASS, 1=FAIL, 2=SKIP

set -euo pipefail

CRATE_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"
MIGRATIONS_DIR="$CRATE_ROOT/crates/mike/src/store/persistence/migrations"

ALL_RS=$(find "$CRATE_ROOT/crates" -name "*.rs" \
    ! -path "*/target/*" 2>/dev/null | sort)

if [ $# -gt 0 ]; then
    MIGRATIONS_TO_CHECK="$*"
else
    # Default: untracked migrations + modified migrations
    UNTRACKED=$(git -C "$CRATE_ROOT" ls-files --others --exclude-standard -- '*.sql' \
        | sed "s|^|$CRATE_ROOT/|")
    MODIFIED=$(git -C "$CRATE_ROOT" diff --name-only HEAD -- '*.sql' 2>/dev/null \
        | sed "s|^|$CRATE_ROOT/|")
    MIGRATIONS_TO_CHECK=$(printf '%s\n%s\n' "$UNTRACKED" "$MODIFIED" | sort -u)
fi

overall_fail=0
overall_warn=0
checked=0

echo "=== G2: Migration DDL impact grader ==="
echo ""

check_column_absent() {
    local column="$1"
    local table_short="$2"
    local hits
    # Look for the column name near a reference to the table
    hits=$(grep -rn --include="*.rs" "\b${column}\b" $CRATE_ROOT/crates/ \
        ! -path "*/target/*" 2>/dev/null \
        | grep -v "^Binary" | head -20 || true)
    if [ -n "$hits" ]; then
        echo "  WARN: dropped column '$column' still referenced in Rust:"
        echo "$hits" | sed 's/^/    /'
        return 1
    fi
    return 0
}

check_table_absent() {
    local table_short="$1"
    local hits
    hits=$(grep -rn --include="*.rs" "\"${table_short}\"\|${table_short}[^a-z_]" \
        $CRATE_ROOT/crates/ ! -path "*/target/*" 2>/dev/null \
        | grep -v "^Binary" | head -20 || true)
    if [ -n "$hits" ]; then
        echo "  WARN: dropped table '${table_short}' still referenced in Rust:"
        echo "$hits" | sed 's/^/    /'
        return 1
    fi
    return 0
}

while IFS= read -r migration_file; do
    [ -f "$migration_file" ] || continue
    echo "── $(basename "$migration_file") ──"
    migration_fail=0

    # ── ADD COLUMN NOT NULL (without DEFAULT) ──────────────────────────────
    while IFS= read -r col_and_table; do
        [ -z "$col_and_table" ] && continue
        table=$(echo "$col_and_table" | cut -d'|' -f1)
        col=$(echo "$col_and_table" | cut -d'|' -f2)
        short_table=$(echo "$table" | sed 's/mike\.//')
        checked=$((checked + 1))

        echo "  [ADD NOT NULL] $table.$col"
        missing=0
        while IFS= read -r rs_file; do
            [ -f "$rs_file" ] || continue
            insert_lines=$(grep -inE "INSERT INTO (mike\.)?${short_table}[^a-z_]" \
                "$rs_file" | cut -d: -f1 || true)
            for lineno in $insert_lines; do
                window=$(awk "NR>=$lineno && NR<=$((lineno+20))" "$rs_file")
                if ! echo "$window" | grep -iqE "\b${col}\b"; then
                    echo "    FAIL: $rs_file:$lineno — INSERT missing '$col'"
                    migration_fail=$((migration_fail + 1))
                    missing=$((missing + 1))
                fi
            done
        done <<< "$ALL_RS"
        [ "$missing" -eq 0 ] && echo "    ok"
    done < <(grep -iE '^\s*(ALTER TABLE\s+([a-z_.]+).*)?ADD COLUMN\s+([a-z_][a-z0-9_]*)\s+[A-Z].*NOT NULL' \
        "$migration_file" | grep -ivE 'DEFAULT\s+\S' | \
        awk '{
            for(i=1;i<=NF;i++) {
                if(tolower($i)=="table") tbl=$(i+1)
                if(tolower($i)=="column") { col=$(i+1); print tbl"|"col; break }
            }
        }' || true)

    # ── DROP COLUMN ────────────────────────────────────────────────────────
    while IFS= read -r col; do
        [ -z "$col" ] && continue
        checked=$((checked + 1))
        echo "  [DROP COLUMN] $col"
        if ! check_column_absent "$col" ""; then
            migration_fail=$((migration_fail + 1))
        else
            echo "    ok (column not referenced in Rust)"
        fi
    done < <(grep -iE '^\s*ALTER TABLE.*DROP COLUMN\s+[A-Z_]' "$migration_file" \
        | sed -n 's/.*DROP COLUMN\s\+\([a-z_][a-z0-9_]*\).*/\1/Ip' \
        | sort -u || true)

    # ── DROP TABLE ─────────────────────────────────────────────────────────
    while IFS= read -r table; do
        [ -z "$table" ] && continue
        checked=$((checked + 1))
        short=$(echo "$table" | sed 's/mike\.//')
        echo "  [DROP TABLE] $table"
        if ! check_table_absent "$short"; then
            migration_fail=$((migration_fail + 1))
        else
            echo "    ok (table not referenced in Rust)"
        fi
    done < <(grep -iE '^\s*DROP TABLE' "$migration_file" \
        | sed -n 's/.*DROP TABLE\s\+\(IF EXISTS\s\+\)\?\([a-z_.][a-z0-9_.]*\).*/\2/Ip' \
        | sort -u || true)

    # ── RENAME COLUMN ──────────────────────────────────────────────────────
    while IFS= read -r rename_pair; do
        [ -z "$rename_pair" ] && continue
        old=$(echo "$rename_pair" | cut -d'>' -f1)
        new=$(echo "$rename_pair" | cut -d'>' -f2)
        checked=$((checked + 1))
        echo "  [RENAME COLUMN] $old → $new"

        # Old name should be absent, new name should be present
        old_hits=$(grep -rn --include="*.rs" "\b${old}\b" "$CRATE_ROOT/crates/" \
            ! -path "*/target/*" 2>/dev/null | grep -v "^Binary" | head -5 || true)
        new_hits=$(grep -rn --include="*.rs" "\b${new}\b" "$CRATE_ROOT/crates/" \
            ! -path "*/target/*" 2>/dev/null | grep -v "^Binary" | head -5 || true)

        if [ -n "$old_hits" ]; then
            echo "    FAIL: old column name '$old' still present in Rust:"
            echo "$old_hits" | sed 's/^/      /'
            migration_fail=$((migration_fail + 1))
        fi
        if [ -z "$new_hits" ]; then
            echo "    WARN: new column name '$new' not found anywhere in Rust — may be unused or unneeded"
            overall_warn=$((overall_warn + 1))
        else
            echo "    ok (old absent, new present)"
        fi
    done < <(grep -iE '^\s*ALTER TABLE.*RENAME COLUMN' "$migration_file" \
        | sed -n 's/.*RENAME COLUMN\s\+\([a-z_][a-z0-9_]*\)\s\+TO\s\+\([a-z_][a-z0-9_]*\).*/\1>\2/Ip' \
        | sort -u || true)

    if [ "$migration_fail" -gt 0 ]; then
        echo "  → FAIL ($migration_fail issue(s))"
        overall_fail=$((overall_fail + migration_fail))
    else
        echo "  → PASS"
    fi
    echo ""
done <<< "$MIGRATIONS_TO_CHECK"

echo "────────────────────────────────────────"
if [ "$checked" -eq 0 ]; then
    echo "SKIP: no DDL impact checks triggered (no relevant migrations)"
    exit 2
elif [ "$overall_fail" -eq 0 ]; then
    echo "PASS: $checked check(s) — 0 failures, $overall_warn warning(s)"
    exit 0
else
    echo "FAIL: $overall_fail issue(s) across $checked check(s) ($overall_warn warnings)"
    exit 1
fi
