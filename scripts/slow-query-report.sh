#!/usr/bin/env bash
# slow-query-report.sh — Analyze slow Postgres queries from the claude-proxy container.
#
# For each slow query captured by log_min_duration_statement (currently 100ms):
#   1. Shows duration + full SQL
#   2. Runs EXPLAIN (ANALYZE, BUFFERS) — wrapped in BEGIN/ROLLBACK so no side effects
#   3. Lists indexes on every referenced table
#   4. Greps crates/ to find the Rust call site
#   5. Emits a one-line verdict
#
# Usage:
#   ./scripts/slow-query-report.sh              # last 15 minutes (default)
#   ./scripts/slow-query-report.sh --since 1h   # custom time window
#   ./scripts/slow-query-report.sh --since 24h  # full day
#   ./scripts/slow-query-report.sh --append     # also append findings to the worklog thread
#
# The --append flag writes structured entries to:
#   docs/threads/slow-queries/worklog.md

set -euo pipefail

SINCE="15m"
APPEND=false
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKLOG="$PROJECT_ROOT/docs/threads/slow-queries/worklog.md"

# Capture DB = MIKE_DATABASE_URL (the repo's single DB source of truth, in .env).
# DATABASE_URL is sqlx-tooling-only and is NOT set at runtime — don't read it here.
if [[ -z "${MIKE_DATABASE_URL:-}" && -f "$PROJECT_ROOT/.env" ]]; then
  set -a; source "$PROJECT_ROOT/.env"; set +a
fi
DB_URL="${MIKE_DATABASE_URL:?set MIKE_DATABASE_URL (expected in .env)}"

# ── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --since) SINCE="$2"; shift 2 ;;
    --append) APPEND=true; shift ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
      exit 0
      ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

# ── Helpers ──────────────────────────────────────────────────────────────────
psql_run() {
  psql -qAt "$DB_URL" "$@" 2>&1
}

explain_query() {
  local sql="$1"
  # Replace $N placeholders with NULL so EXPLAIN can parse parameterized queries.
  # We use plain EXPLAIN (no ANALYZE) to avoid actually executing DML with NULLs.
  local safe_sql
  safe_sql=$(echo "$sql" | sed -E 's/\$[0-9]+/NULL/g')
  # For SELECT/WITH: safe to EXPLAIN ANALYZE inside a rolled-back tx.
  local first_word
  first_word=$(echo "$sql" | awk '{print toupper($1)}')
  if [[ "$first_word" == "SELECT" || "$first_word" == "WITH" ]]; then
    psql_run -c "BEGIN; EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) $safe_sql; ROLLBACK;" 2>&1 \
      || psql_run -c "EXPLAIN $safe_sql;" 2>&1 \
      || echo "(explain failed)"
  else
    psql_run -c "EXPLAIN $safe_sql;" 2>&1 \
      || echo "(explain failed)"
  fi
}

indexes_for_table() {
  local tbl="$1"
  # Resolve schema — default to mike if unqualified.
  local schema="mike"
  local tablename="$tbl"
  if [[ "$tbl" == *.* ]]; then
    schema="${tbl%%.*}"
    tablename="${tbl##*.}"
  fi
  psql_run -c "
    SELECT '  ' || indexname || ': ' || indexdef
    FROM pg_indexes
    WHERE schemaname = '$schema' AND tablename = '$tablename'
    ORDER BY indexname;
  " 2>&1 || echo "  (could not fetch indexes)"
}

grep_codebase() {
  local sql="$1"
  # Build a search term from words 3-6 of the SQL (skip SELECT/FROM/etc).
  local term
  term=$(echo "$sql" | tr -s '[:space:]' ' ' | cut -d' ' -f3-6 \
    | sed -E 's/[();,]//g' | xargs)
  [[ -z "$term" ]] && echo "  (no search term)" && return
  local hits
  hits=$(grep -rn --include="*.rs" --include="*.sql" -F "$term" \
    "$PROJECT_ROOT/crates/" 2>/dev/null \
    | sed "s|$PROJECT_ROOT/||" | head -5 || true)
  if [[ -n "$hits" ]]; then
    echo "$hits"
  else
    echo "  (not found in crates/ — may be a migration or dynamic SQL)"
  fi
}

verdict_from_explain() {
  local explain="$1"
  local is_ddl="$2"
  if [[ "$is_ddl" == "true" ]]; then
    echo "DDL at startup — expected one-time cost, not a runtime concern"
  elif echo "$explain" | grep -qi "Seq Scan"; then
    echo "SEQUENTIAL SCAN — likely missing index on the filter column"
  elif echo "$explain" | grep -qi "Nested Loop" && echo "$explain" | grep -qi "Seq Scan"; then
    echo "Nested loop + seq scan — missing FK or join index"
  elif echo "$explain" | grep -qi "explain failed"; then
    echo "Could not analyze — parameterized query, review manually"
  else
    echo "Plan looks reasonable — check row-count estimates vs actuals above"
  fi
}

# ── Fetch slow query lines ───────────────────────────────────────────────────
# Slow-query log lines came from the LOCAL Docker Postgres container's logs. That
# local container has been removed — the capture DB is now a remote/managed
# Postgres at MIKE_DATABASE_URL — so `docker logs` is no longer a valid source.
if ! docker logs claude-proxy-postgres --since "$SINCE" >/dev/null 2>&1; then
  echo "slow-query-report: the local Docker Postgres ('claude-proxy-postgres') is gone." >&2
  echo "This tool parsed that container's slow-query logs; the capture DB is now remote" >&2
  echo "(MIKE_DATABASE_URL). Read slow queries from the server log or pg_stat_statements" >&2
  echo "instead. (The EXPLAIN path below still connects via MIKE_DATABASE_URL.)" >&2
  exit 2
fi
LINES=$(docker logs claude-proxy-postgres --since "$SINCE" 2>&1 \
  | grep -E "LOG:.*duration: [0-9]+\.[0-9]+ ms.*(statement|execute):" || true)

if [[ -z "$LINES" ]]; then
  echo "No slow queries found in the last $SINCE."
  exit 0
fi

COUNT=$(echo "$LINES" | wc -l | tr -d ' ')
echo "Found $COUNT slow query line(s) in the last $SINCE."
echo ""

WORKLOG_ENTRIES=""

while IFS= read -r line; do
  # ── Parse log line ─────────────────────────────────────────────────────────
  TIMESTAMP=$(echo "$line" | grep -oE "^[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+ UTC" || echo "unknown")
  DURATION=$(echo "$line" | grep -oE "duration: [0-9]+\.[0-9]+ ms" | grep -oE "[0-9]+\.[0-9]+ ms" || echo "?")
  # statement: or execute <name>:
  SQL=$(echo "$line" | sed -E 's/.*( statement| execute [^:]+): //' | sed 's/;*$//' || true)
  [[ -z "$SQL" ]] && continue

  FIRST_WORD=$(echo "$SQL" | awk '{print toupper($1)}')
  IS_DDL=false
  case "$FIRST_WORD" in
    CREATE|DROP|ALTER|TRUNCATE|VACUUM|REINDEX|COMMENT|GRANT|REVOKE) IS_DDL=true ;;
  esac

  # ── EXPLAIN (collected for worklog; not printed in full) ──────────────────
  if $IS_DDL; then
    EXPLAIN_OUT="(DDL — no query plan)"
  else
    EXPLAIN_OUT=$(explain_query "$SQL")
  fi

  # ── Indexes ────────────────────────────────────────────────────────────────
  TABLES=$(echo "$SQL" \
    | grep -oiE "(FROM|JOIN|UPDATE|INTO)\s+([a-zA-Z_][a-zA-Z0-9_.]*)" \
    | awk '{print $NF}' \
    | grep -viE "^(select|where|set|values|lateral|only)$" \
    | sort -u || true)

  INDEX_SUMMARY=""
  if [[ -n "$TABLES" ]]; then
    while IFS= read -r tbl; do
      IDX=$(indexes_for_table "$tbl" | grep -c "idx\|pkey\|_key" 2>/dev/null || echo 0)
      INDEX_SUMMARY="${INDEX_SUMMARY}${tbl}(${IDX} idx) "
    done <<< "$TABLES"
  fi

  # ── Codebase location ──────────────────────────────────────────────────────
  HITS=$(grep_codebase "$SQL")
  LOCATION=$(echo "$HITS" | head -1 | sed 's/^[[:space:]]*//')

  # ── Verdict ────────────────────────────────────────────────────────────────
  VERDICT=$(verdict_from_explain "$EXPLAIN_OUT" "$IS_DDL")

  # ── Compact stdout output ─────────────────────────────────────────────────
  echo "[$TIMESTAMP]  ${DURATION}"
  echo "  sql     : $(echo "$SQL" | cut -c1-120)"
  echo "  tables  : ${INDEX_SUMMARY:-none}"
  echo "  src     : $LOCATION"
  echo "  verdict : $VERDICT"
  echo ""

  # ── Build worklog entry ────────────────────────────────────────────────────
  WORKLOG_ENTRIES="${WORKLOG_ENTRIES}
### ${TIMESTAMP} — ${DURATION}
\`\`\`sql
${SQL}
\`\`\`
**EXPLAIN:**
\`\`\`
${EXPLAIN_OUT}
\`\`\`
**Tables:** \`${TABLES:-none detected}\`
**Codebase:** ${HITS}
**Verdict:** ${VERDICT}
"

done <<< "$LINES"

# ── Append to worklog ─────────────────────────────────────────────────────────
if $APPEND && [[ -n "$WORKLOG_ENTRIES" ]]; then
  # Insert before the closing comment in the worklog
  printf '%s\n' "$WORKLOG_ENTRIES" >> "$WORKLOG"
  echo "Appended $COUNT entr(ies) to $WORKLOG"
fi
