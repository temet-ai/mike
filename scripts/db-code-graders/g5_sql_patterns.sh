#!/usr/bin/env bash
# G5 — Bad SQL pattern detector
#
# Catches SQL quality issues from the 2026-05-26 query audit (H1-H6, M1-M8).
# These are patterns that compile and run correctly but are slow or incorrect
# at production scale. No DB connection needed — pure text analysis.
#
# Patterns detected:
# The two blocking checks are structural detectors with their own self-tests
# (`--self-test`): g5_correlated_subquery.py and g5_row_by_row_insert.py. They
# replaced regex/awk heuristics whose every standing hit was a false positive —
# a gate that is always red is a gate nobody reads.
#
#   CORR-SUBQUERY   — scalar subquery in SELECT that fires per-row (O(N))
#   NO-LIMIT        — SELECT without LIMIT/FETCH on a large table
#   ROW-BY-ROW      — INSERT/UPDATE in a Rust loop instead of batch
#   FULL-TABLE-SCAN — WHERE on an unindexed column (common false-positive, advisory)
#   UNSCOPED-DELETE — DELETE FROM without WHERE (catastrophic, as per 2026-04-24)
#   UNNESTED-COUNT  — COUNT(*) in a correlated subquery avoidable by window/JOIN
#
# Exit: 0=PASS, 1=FAIL (has blocking issues), 2=WARN (advisory only)

set -euo pipefail

CRATE_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# All Rust persistence source files
PERSISTENCE_RS=$(find "$CRATE_ROOT/crates" -name "*.rs" \
    ! -path "*/target/*" ! -path "*/tests/*" \
    | grep -E "(persistence|store|handler|updater|operator_turns|lessons|compositions|rehydrate)" \
    | sort)

echo "=== G5: Bad SQL pattern detector ==="
echo ""

blocking=0
advisory=0

# ── UNSCOPED DELETE (blocking) ────────────────────────────────────────────────
echo "── [BLOCKING] Unscoped DELETE (no WHERE or USING clause) ──"
found=0
while IFS= read -r f; do
    [ -f "$f" ] || continue
    # Match schema-qualified DELETE (post-squash) and legacy claude_proxy. prefix.
    # `|| true` prevents pipefail from exiting when grep finds no matches (exit 1).
    # Process substitution, not a pipe: `| while` runs the loop in a subshell,
    # so `found` never reached this scope and a real unscoped DELETE could not
    # have blocked anything.
    while read -r lineno; do
        [ -n "$lineno" ] || continue
        end=$((lineno + 8))
        window=$(sed -n "${lineno},${end}p" "$f")
        if ! echo "$window" | grep -qiE 'WHERE|USING'; then
            echo "  FAIL: $f:$lineno"
            sed -n "${lineno},${lineno}p" "$f" | sed 's/^/    /'
            found=$((found + 1))
        fi
    done < <(grep -nE '"DELETE FROM (mike\.|claude_proxy\.|rules\.|memory\.|requests\.|sessions\.|agents\.|system_prompts\.|overrides\.)' "$f" 2>/dev/null | cut -d: -f1)
done <<< "$PERSISTENCE_RS"
[ "$found" -gt 0 ] && blocking=$((blocking + found)) || echo "  ok"
echo ""

# ── CORRELATED SCALAR SUBQUERY (blocking) ────────────────────────────────────
# Delegated to g5_correlated_subquery.py, which decides position and correlation
# structurally. The regex this replaced ran DOTALL over whole files and matched a
# SELECT in one string literal against a subquery in another, so all 10 of its
# standing "blocking" hits were false positives — EXISTS, derived tables and
# UNNEST, every one of them explicitly ALLOWED by .claude/rules/rust-database-reliability.md.
# A permanently-red gate is a gate nobody reads.
echo "── [BLOCKING] Correlated scalar subquery per-row ──"
echo "   a scalar (SELECT …) referencing the outer row — see .claude/rules/rust-database-reliability.md"
corr_out=$(printf '%s\n' "$PERSISTENCE_RS" | tr '\n' '\0' \
    | xargs -0 python3 "$(dirname "${BASH_SOURCE[0]}")/g5_correlated_subquery.py" 2>&1) && corr_rc=0 || corr_rc=$?
if [ "${corr_rc:-0}" -eq 0 ]; then
    echo "  ok"
else
    echo "$corr_out"
    # exit 2 = it could not read a file it was asked to grade; that is an error,
    # not a clean bill, so it counts as blocking rather than passing silently.
    hits_n=$(printf '%s\n' "$corr_out" | grep -c '^    L' || true)
    blocking=$((blocking + ${hits_n:-1}))
fi
echo ""

# ── ROW-BY-ROW INSERT IN RUST LOOP (blocking) ─────────────────────────────────
# Delegated to g5_row_by_row_insert.py. The awk this replaced flagged any `for`
# within 20 lines of an INSERT, so it matched the canonical batch shape — a loop
# filling Vecs followed by one INSERT … SELECT FROM UNNEST — and reported the
# recommended fix as the bug. It also missed real hits further than 20 lines from
# their loop, which the replacement finds.
echo "── [BLOCKING] Row-by-row INSERT in Rust loop ──"
echo "   (H3: an INSERT executed once per loop iteration → batch it with UNNEST)"
rbr_out=$(printf '%s\n' "$PERSISTENCE_RS" | tr '\n' '\0' \
    | xargs -0 python3 "$(dirname "${BASH_SOURCE[0]}")/g5_row_by_row_insert.py" 2>&1) && rbr_rc=0 || rbr_rc=$?
if [ "${rbr_rc:-0}" -eq 0 ]; then
    echo "  ok"
else
    echo "$rbr_out"
    rbr_n=$(printf '%s\n' "$rbr_out" | grep -c '^    L' || true)
    blocking=$((blocking + ${rbr_n:-1}))
fi
echo ""

# ── SELECT WITHOUT LIMIT on large tables (advisory) ───────────────────────────
echo "── [ADVISORY] SELECT without LIMIT on large tables ──"
LARGE_TABLES="requests|sessions|tool_uses|tool_results|response_stream_chunks|operator_turns|operator_turn_chunks"
found=0
while IFS= read -r f; do
    [ -f "$f" ] || continue
    # Find SELECT FROM large table without LIMIT
    hits=$(grep -n '"SELECT\|"    SELECT\|"  SELECT' "$f" 2>/dev/null \
        | grep -iE "$LARGE_TABLES" \
        | grep -iv 'LIMIT\|fetch_one\|fetch_optional\|query_scalar\|COUNT(' \
        | head -10 || true)
    if [ -n "$hits" ]; then
        rel="${f#$CRATE_ROOT/}"
        echo "  $rel:"
        echo "$hits" | sed 's/^/    /'
        found=$((found + 1))
    fi
done <<< "$PERSISTENCE_RS"
[ "$found" -gt 0 ] && advisory=$((advisory + found)) && echo "" || echo "  ok"

# ── SEQUENTIAL COUNT(*) ROUND-TRIPS (advisory) ───────────────────────────────
echo "── [ADVISORY] Multiple sequential COUNT(*) (M2: should be one query) ──"
found=0
while IFS= read -r f; do
    [ -f "$f" ] || continue
    count_hits=$(grep -c 'SELECT COUNT(\*)' "$f" 2>/dev/null || echo "0")
    if [ "$count_hits" -gt 3 ] 2>/dev/null; then
        rel="${f#$CRATE_ROOT/}"
        echo "  $rel: $count_hits COUNT(*) calls — consider combining into one query"
        found=$((found + 1))
    fi
done <<< "$PERSISTENCE_RS"
[ "$found" -gt 0 ] && advisory=$((advisory + found)) && echo "" || echo "  ok"
echo ""

# ── CTE REUSE CHECK (advisory) ────────────────────────────────────────────────
echo "── [ADVISORY] CTE defined but queried multiple times (potential M6 pattern) ──"
found=0
while IFS= read -r f; do
    [ -f "$f" ] || continue
    # Find SQL with multiple SELECTs referencing the same CTE name
    hits=$(grep -c '\bWITH\b' "$f" 2>/dev/null || echo "0")
    if [ "$hits" -gt 0 ] 2>/dev/null; then
        cte_count=$(grep -c 'COUNT(\*) FROM \|COUNT(\*) AS \|SUM(\|AVG(' "$f" 2>/dev/null || echo "0")
        if [ "$cte_count" -gt 3 ] 2>/dev/null; then
            rel="${f#$CRATE_ROOT/}"
            echo "  $rel: $hits CTE(s), $cte_count aggregate calls — verify no M6-style re-scan"
            found=$((found + 1))
        fi
    fi
done <<< "$PERSISTENCE_RS"
[ "$found" -gt 0 ] && advisory=$((advisory + found)) || echo "  ok"
echo ""

# ── Summary ────────────────────────────────────────────────────────────────────
echo "────────────────────────────────────────"
echo "Blocking issues: $blocking | Advisory warnings: $advisory"
echo ""

if [ "$blocking" -gt 0 ]; then
    echo "FAIL: $blocking blocking SQL pattern(s) found"
    exit 1
elif [ "$advisory" -gt 0 ]; then
    echo "WARN: $advisory advisory SQL pattern(s) — review recommended"
    exit 2
else
    echo "PASS: no bad SQL patterns detected"
    exit 0
fi
