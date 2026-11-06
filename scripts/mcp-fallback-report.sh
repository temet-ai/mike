#!/usr/bin/env bash
# MCP fallback-adoption report — measures proxy-data access hygiene.
# Compares mcp__mike__* calls vs SQL/curl fallbacks (proxy-q / psql / curl :9989).
# Usage: ./scripts/mcp-fallback-report.sh [since_interval] [project_path]

set -euo pipefail

# Capture DB = MIKE_DATABASE_URL (the repo's single DB source of truth, in .env).
# DATABASE_URL is sqlx-tooling-only and is NOT set at runtime — don't read it here.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${MIKE_DATABASE_URL:-}" && -f "$ROOT/.env" ]]; then
  set -a; source "$ROOT/.env"; set +a
fi
DB_URL="${MIKE_DATABASE_URL:?set MIKE_DATABASE_URL (expected in .env)}"
SINCE="${1:-7 days}"
PROJECT="${2:-projects/temet/temet-proxy}"

if ! psql "$DB_URL" -c "SELECT 1" >/dev/null 2>&1; then
  echo "ERROR: Cannot connect to $DB_URL" >&2
  exit 1
fi

# ── helpers ────────────────────────────────────────────────────────────────────

bar() {
  local pct=${1%.*}     # truncate to int
  local filled=$(( pct / 5 ))
  local empty=$(( 20 - filled ))
  local b=""
  local i
  for (( i=0; i<filled; i++ )); do b+="█"; done
  for (( i=0; i<empty;  i++ )); do b+="░"; done
  printf "%s" "$b"
}

q() {   # run a tuple-only query, strip whitespace
  psql "$DB_URL" -t -A -v "project=$PROJECT" -v "since=$SINCE" 2>/dev/null <<< "$1" | xargs
}

# ── top-line metric ────────────────────────────────────────────────────────────

TOTAL_MCP=$(q "
  SELECT COUNT(*) FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
    AND tu.tool_name LIKE 'mcp__mike__%';")

TOTAL_FALLBACK=$(q "
  SELECT COUNT(*) FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
    AND tu.tool_name = 'Bash'
    AND (
      (tu.input->>'command') ~ 'proxy-q'
      OR (tu.input->>'command') ~ 'psql'
      OR (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)'
    );")

TOTAL=$(( TOTAL_MCP + TOTAL_FALLBACK ))

if [[ $TOTAL -eq 0 ]]; then
  PCT="n/a"
  BAR_STR="────────────────────"
else
  PCT=$(awk "BEGIN { printf \"%.0f\", ($TOTAL_MCP * 100) / $TOTAL }")
  BAR_STR=$(bar "$PCT")
fi

# ── header ─────────────────────────────────────────────────────────────────────

printf "\n"
printf "  MCP fallback-adoption  ·  %s  ·  last %s\n" "$PROJECT" "$SINCE"
printf "  ─────────────────────────────────────────────────────\n"
if [[ $TOTAL -eq 0 ]]; then
  printf "  Adoption  n/a  (no proxy-data accesses recorded)\n"
else
  printf "  Adoption  %3s%%  [%s]  (%s MCP / %s total)\n" \
    "$PCT" "$BAR_STR" "$TOTAL_MCP" "$TOTAL"
fi
printf "  Target: 100%% MCP  ·  zero SQL/curl fallbacks\n"
printf "\n"

# ── breakdown summary (4 rows: MCP total + 3 fallback types) ──────────────────

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Proxy-data access breakdown (summary)'
\set QUIET off

SELECT
  category                                                            AS "Method",
  count                                                               AS "Calls",
  ROUND(count * 100.0 / SUM(count) OVER (), 1)::text || '%'          AS "Share"
FROM (
  SELECT
    CASE
      WHEN tu.tool_name LIKE 'mcp__mike__%'
        THEN '✓  mcp__mike__*  (MCP — target path)'
      WHEN tu.tool_name = 'Bash'
        AND (tu.input->>'command') ~ 'proxy-q'
        THEN '✗  Bash proxy-q  (SQL fallback)'
      WHEN tu.tool_name = 'Bash'
        AND (tu.input->>'command') ~ 'psql'
        THEN '✗  Bash psql  (SQL fallback)'
      WHEN tu.tool_name = 'Bash'
        AND (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)'
        THEN '✗  Bash curl/wget :9989  (HTTP fallback)'
      ELSE NULL
    END AS category,
    COUNT(*) AS count
  FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
  GROUP BY 1
) sub
WHERE category IS NOT NULL
ORDER BY
  CASE WHEN category LIKE '✓%' THEN 0 ELSE 1 END,
  count DESC;
SQL

# ── MCP per-tool detail ────────────────────────────────────────────────────────

printf "\n"

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'MCP tool usage breakdown (top 20 by call count)'
\set QUIET off

SELECT
  tu.tool_name                                                        AS "MCP Tool",
  COUNT(*)                                                            AS "Calls",
  ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER (), 1)::text || '%'    AS "Share of MCP"
FROM tools.uses tu
JOIN requests.requests r ON tu.request_id = r.id
JOIN projects.projects p ON r.project_id = p.id
WHERE p.project_path = :'project'
  AND r.created_at > NOW() - (:'since')::interval
  AND tu.tool_name LIKE 'mcp__mike__%'
GROUP BY tu.tool_name
ORDER BY COUNT(*) DESC
LIMIT 20;
SQL

# ── per-session fallback attribution ──────────────────────────────────────────

printf "\n"

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Fallback calls per session (sessions with ≥1 fallback)'
\set QUIET off

SELECT
  LEFT(s.session_uuid::text, 8)           AS "Session",
  SUM(CASE WHEN (tu.input->>'command') ~ 'proxy-q'                                          THEN 1 ELSE 0 END) AS "proxy-q",
  SUM(CASE WHEN (tu.input->>'command') ~ 'psql'                                             THEN 1 ELSE 0 END) AS "psql",
  SUM(CASE WHEN (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)' THEN 1 ELSE 0 END) AS "curl/wget",
  COUNT(*)                                AS "Total fallbacks",
  to_char(MAX(r.created_at), 'MM-DD HH24:MI') AS "Last seen"
FROM tools.uses tu
JOIN requests.requests r  ON tu.request_id = r.id
JOIN sessions.sessions  s ON r.session_id  = s.id
JOIN projects.projects  p ON r.project_id  = p.id
WHERE p.project_path = :'project'
  AND r.created_at > NOW() - (:'since')::interval
  AND tu.tool_name = 'Bash'
  AND (
    (tu.input->>'command') ~ 'proxy-q'
    OR (tu.input->>'command') ~ 'psql'
    OR (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)'
  )
GROUP BY s.session_uuid
ORDER BY COUNT(*) DESC
LIMIT 15;
SQL

# ── recent fallback commands (detail) ─────────────────────────────────────────

printf "\n"

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Recent fallback commands (last 15)'
\set QUIET off

SELECT
  to_char(r.created_at, 'MM-DD HH24:MI')  AS "At",
  CASE
    WHEN (tu.input->>'command') ~ 'proxy-q'                                          THEN 'proxy-q'
    WHEN (tu.input->>'command') ~ 'psql'                                             THEN 'psql'
    WHEN (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)' THEN 'curl/wget'
  END                                      AS "Type",
  LEFT(tu.input->>'command', 120)          AS "Command"
FROM tools.uses tu
JOIN requests.requests r ON tu.request_id = r.id
JOIN projects.projects p ON r.project_id  = p.id
WHERE p.project_path = :'project'
  AND r.created_at > NOW() - (:'since')::interval
  AND tu.tool_name = 'Bash'
  AND (
    (tu.input->>'command') ~ 'proxy-q'
    OR (tu.input->>'command') ~ 'psql'
    OR (tu.input->>'command') ~ '(curl|wget).*(9989|localhost:9989|127\.0\.0\.1:9989)'
  )
ORDER BY r.created_at DESC
LIMIT 15;
SQL
