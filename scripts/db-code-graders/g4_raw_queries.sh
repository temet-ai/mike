#!/usr/bin/env bash
# G4 — Raw sqlx::query() detector
#
# Finds all `sqlx::query(` and `sqlx::query_as(` calls in persistence-layer
# Rust code that could use the compile-time checked `sqlx::query!()` /
# `sqlx::query_as!()` macros instead.
#
# The checked macros validate:
#   - SQL syntax at compile time
#   - bind parameter count matches the query's $N placeholders
#   - bind parameter types are compatible
#   - result column names and types match the return type
#
# The 20260526 bug: section 3f used `sqlx::query(...)` with 7 binds.
# After the migration added `session_id NOT NULL`, it should have had 8 binds.
# `sqlx::query!()` would have caused a compile error. `sqlx::query()` silently
# produced a runtime constraint violation.
#
# Output: categorised list of raw query call sites, grouped by severity.
#
# Usage:
#   g4_raw_queries.sh                          audit mode (default)
#   g4_raw_queries.sh --commit-scope <cached|worktree>
#
# Audit mode grades the whole working TREE and blocks on any HIGH hit — the
# right answer for run_all_graders.sh, which is asking about the repo rather
# than about one commit.
#
# --commit-scope grades a specific commit, mirroring G3's fix for the same
# class of problem (mike#163): without it, a persistence-layer raw query that
# has sat on main for months blocks every unrelated .rs commit forever, since
# this grader inspects the whole tree rather than the diff. Under
# --commit-scope, a HIGH hit blocks only when the commit's own diff ADDS that
# line (same "added lines only" scope as scripts/fidelity-check); a
# pre-existing hit the commit does not touch is reported as a non-blocking
# advisory instead, so the debt stays visible without freezing unrelated work.
#
# Exit: 0=PASS (no raw queries in persistence layer, or none added by this
#       commit), 1=FAIL, 2=INFO (warnings only)

set -euo pipefail

CRATE_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

commit_scope=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --commit-scope) commit_scope="${2:-cached}"; shift 2 ;;
        *) shift ;;
    esac
done
[[ "$commit_scope" == "worktree" || "$commit_scope" == "cached" ]] || commit_scope="${commit_scope:+cached}"

# Lines this commit ADDS, keyed by absolute file path — built once, up front,
# so the HIGH-scan loop below can classify each hit as newly introduced or
# pre-existing without re-diffing per hit.
declare -A ADDED_LINES_FOR_FILE=()
if [ -n "$commit_scope" ]; then
    scope_args=()
    [ "$commit_scope" = "cached" ] && scope_args=(--cached)
    # Renames are read with -M against the old path: diffing the new path alone
    # reports every line of a moved file as added (the crate split moves hundreds).
    diff_entries=$(git -C "$CRATE_ROOT" diff "${scope_args[@]}" -M --name-status -- '*.rs' 2>/dev/null || true)
    while IFS=$'\t' read -r status first second; do
        [ -z "$status" ] && continue
        paths=("$first")
        f="$first"
        if [ "${status#R}" != "$status" ]; then
            paths=("$first" "$second")
            f="$second"
        fi
        abs="$CRATE_ROOT/$f"
        added=$(git -C "$CRATE_ROOT" diff "${scope_args[@]}" -M -- "${paths[@]}" 2>/dev/null | grep -E '^\+[^+]' | sed 's/^\+//' || true)
        ADDED_LINES_FOR_FILE["$abs"]="$added"
    done <<< "$diff_entries"
fi

is_newly_added() {
    local abs_file="$1" line_content="$2" added
    [ -z "$commit_scope" ] && return 1
    added="${ADDED_LINES_FOR_FILE[$abs_file]:-}"
    [ -z "$added" ] && return 1
    grep -qxF "$line_content" <<< "$added"
}

# Files in persistence scope — where compile-time checking matters most
PERSISTENCE_FILES=$(find "$CRATE_ROOT/crates" -name "*.rs" \
    ! -path "*/target/*" \
    | grep -E "(persistence|store|handler|updater)" \
    | grep -vE "(/tests/|/tests\.rs$|_tests\.rs$|/test_support\.rs$)" \
    | sort)
# The second grep drops test-only modules that live in their own files. The
# 500-line file ceiling (scripts/lint-rust-size) moved most `#[cfg(test)] mod
# tests` blocks out of their parent into `<name>_tests.rs`, `tests.rs`, or a
# `tests/` directory; the inline-block skip in the awk below cannot see those,
# and raw queries in test seed data are acceptable per the severity legend.

# All Rust source files
ALL_RS=$(find "$CRATE_ROOT/crates" -name "*.rs" \
    ! -path "*/target/*" ! -path "*/tests/*" \
    | grep -vE "(/tests\.rs$|_tests\.rs$|/test_support\.rs$)" \
    | sort)

echo "=== G4: Raw sqlx::query() detector ==="
echo ""
echo "Severity tiers:"
echo "  HIGH   — persistence layer: these queries modify the schema-bound tables"
echo "  MEDIUM — other production code: should migrate to checked macros"
echo "  OK     — tests: raw queries are acceptable (dynamic test data)"
echo ""

high_count=0
medium_count=0
blocking_count=0

# ── HIGH: persistence layer ───────────────────────────────────────────────────
echo "── HIGH (persistence layer) ──"
echo ""

while IFS= read -r rs_file; do
    [ -f "$rs_file" ] || continue
    # Production-only scan: exclude the trailing `#[cfg(test)] mod tests {`
    # block (raw queries in test seed data are acceptable per the severity
    # legend above) and any line carrying an explicit `grader-allow:` marker
    # (documented exceptions — e.g. pgvector `$N::vector` casts that
    # sqlx::query! cannot type-check). NR keeps real file line numbers.
    # ASSUMPTION: the `#[cfg(test)] mod tests` block is the last item in the
    # file (the Rust convention — verified across the persistence layer). A
    # production query placed AFTER it would be missed. A brace-depth reset is
    # deliberately NOT used: SQL string literals carry `{`/`}` (e.g.
    # `'{}'::jsonb`) that would corrupt naive depth counting.
    hits=$(awk '
        /^[[:space:]]*mod tests[[:space:]]*\{/ { intest=1 }
        intest { next }
        /grader-allow/ { next }
        /^[[:space:]]*\/\// { next }
        /sqlx::query(_as)?(\(|::<)/ && $0 !~ /sqlx::query!|query_builder|query_scalar!|query_as!/ { printf "%d:%s\n", NR, $0 }
    ' "$rs_file" 2>/dev/null | head -30 || true)
    if [ -n "$hits" ]; then
        rel="${rs_file#$CRATE_ROOT/}"
        echo "  $rel:"
        while IFS= read -r hit; do
            lineno=$(echo "$hit" | cut -d: -f1)
            content=$(echo "$hit" | cut -d: -f2-)
            snippet=$(echo "$content" | sed 's/^\s*//')
            # Extract the table name if present in the query string
            table=$(echo "$snippet" | grep -oiE "(INSERT INTO|UPDATE|DELETE FROM|SELECT.*FROM)\s+[a-z_.]+" \
                | head -1 | awk '{print $NF}' || true)
            tag=""
            if [ -n "$commit_scope" ]; then
                if is_newly_added "$rs_file" "$content"; then
                    tag="  [added by this commit]"
                    blocking_count=$((blocking_count + 1))
                else
                    tag="  [pre-existing]"
                fi
            fi
            echo "    L$lineno: $snippet$tag"
            [ -n "$table" ] && echo "         → table: $table"
        done <<< "$hits"
        high_count=$((high_count + $(echo "$hits" | wc -l | tr -d ' ')))
        echo ""
    fi
done <<< "$PERSISTENCE_FILES"

[ "$high_count" -eq 0 ] && echo "  (none — all persistence queries use checked macros)" && echo ""

# ── MEDIUM: other production code ─────────────────────────────────────────────
echo "── MEDIUM (other production Rust) ──"
echo ""

while IFS= read -r rs_file; do
    [ -f "$rs_file" ] || continue
    # Skip persistence files (already counted as HIGH) and tests
    echo "$PERSISTENCE_FILES" | grep -q "$rs_file" && continue

    hits=$(awk '
        /^[[:space:]]*mod tests[[:space:]]*\{/ { intest=1 }
        intest { next }
        /grader-allow/ { next }
        /^[[:space:]]*\/\// { next }
        /sqlx::query(_as)?(\(|::<)/ && $0 !~ /sqlx::query!|query_builder|query_scalar!|query_as!/ { printf "%d:%s\n", NR, $0 }
    ' "$rs_file" 2>/dev/null | head -10 || true)
    if [ -n "$hits" ]; then
        rel="${rs_file#$CRATE_ROOT/}"
        echo "  $rel:"
        echo "$hits" | sed "s/^/    /"
        medium_count=$((medium_count + $(echo "$hits" | wc -l | tr -d ' ')))
        echo ""
    fi
done <<< "$ALL_RS"

[ "$medium_count" -eq 0 ] && echo "  (none)" && echo ""

# ── Summary ────────────────────────────────────────────────────────────────────
echo "────────────────────────────────────────"
total=$((high_count + medium_count))
echo "Raw sqlx::query() sites: $total total ($high_count HIGH, $medium_count MEDIUM)"
[ -n "$commit_scope" ] && [ "$high_count" -gt 0 ] && \
    echo "  of which $blocking_count HIGH site(s) were added by this commit"
echo ""

if [ -n "$commit_scope" ]; then
    if [ "$blocking_count" -gt 0 ]; then
        echo "FAIL: $blocking_count persistence-layer raw sqlx::query() call site(s) added by this commit."
        echo "  Migrate to sqlx::query!() to catch bind mismatches at compile time, or add a"
        echo "  documented // grader-allow: <reason> marker for a genuine exception."
        exit 1
    fi
    if [ "$high_count" -gt 0 ]; then
        echo "PASS (advisory): $high_count pre-existing HIGH site(s) found, none touched by this commit."
        echo "  Not this commit's to fix — it belongs to whoever added it (mike#163)."
        exit 0
    fi
    if [ "$medium_count" -gt 0 ]; then
        echo "WARN: $medium_count non-persistence queries lack compile-time checking. Not blocking."
        exit 0
    fi
    echo "PASS: no raw sqlx queries added by this commit"
    exit 0
fi

if [ "$high_count" -gt 0 ]; then
    echo "FAIL: $high_count persistence-layer queries lack compile-time checking."
    echo "  Migrate to sqlx::query!() to catch bind mismatches at compile time."
    echo "  Requires: SQLX_OFFLINE=true + sqlx prepare (generates .sqlx/ cache)"
    exit 1
elif [ "$medium_count" -gt 0 ]; then
    echo "WARN: $medium_count non-persistence queries lack compile-time checking."
    echo "  Not blocking, but consider migration. Exit 2."
    exit 2
else
    echo "PASS: all sqlx queries use compile-time checked macros"
    exit 0
fi
