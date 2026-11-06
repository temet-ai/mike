#!/usr/bin/env bash
# ast-grep compliance report — compares structural-search tool usage.
# Usage: ./scripts/ast-grep-compliance-report.sh [since_interval] [project_path]

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

q() {   # run a tuple-only query, strip whitespace (uses stdin for variable interpolation)
  psql "$DB_URL" -t -A -v "project=$PROJECT" -v "since=$SINCE" 2>/dev/null <<< "$1" | xargs
}

# ── compliance rate (top-line metric) ──────────────────────────────────────────

TOTAL_AST=$(q "
  SELECT COUNT(*) FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
    AND tu.tool_name LIKE 'mcp__ast-grep__%';")

TOTAL_GREP=$(q "
  SELECT COUNT(*) FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
    AND tu.tool_name = 'Bash'
    AND (
      (tu.input->>'command') ~ 'grep.+\.(rs|ts|svelte|tsx)'
      OR (
        (tu.input->>'command') ~ 'grep.+(crates/|frontend/src/)'
        AND (tu.input->>'command') !~ 'grep.+\.(toml|md|json|sh|txt|lock)'
      )
    );")

TOTAL_LSP_NAV=$(q "
  SELECT COUNT(*) FROM tools.uses tu
  JOIN requests.requests r ON tu.request_id = r.id
  JOIN projects.projects p ON r.project_id = p.id
  WHERE p.project_path = :'project'
    AND r.created_at > NOW() - (:'since')::interval
    AND tu.tool_name = 'LSP'
    AND tu.operation IN ('workspaceSymbol', 'documentSymbol');")

TOTAL_STRUCTURAL=$(( TOTAL_AST + TOTAL_GREP + TOTAL_LSP_NAV ))

if [[ $TOTAL_STRUCTURAL -eq 0 ]]; then
  PCT="n/a"
  BAR_STR="────────────────────"
else
  PCT=$(awk "BEGIN { printf \"%.0f\", ($TOTAL_AST * 100) / $TOTAL_STRUCTURAL }")
  BAR_STR=$(bar "$PCT")
fi

# ── header ─────────────────────────────────────────────────────────────────────

printf "\n"
printf "  ast-grep compliance  ·  %s  ·  last %s\n" "$PROJECT" "$SINCE"
printf "  ─────────────────────────────────────────────────────────\n"
if [[ $TOTAL_STRUCTURAL -eq 0 ]]; then
  printf "  Compliance  n/a  (no structural searches recorded)\n"
else
  printf "  Compliance  %3s%%  [%s]  (%s ast-grep / %s total)\n" \
    "$PCT" "$BAR_STR" "$TOTAL_AST" "$TOTAL_STRUCTURAL"
fi
printf "\n"

# ── breakdown table ────────────────────────────────────────────────────────────

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Search tool breakdown'
\set QUIET off

SELECT
  category                                                            AS "Tool / pattern",
  count                                                               AS "Calls",
  ROUND(count * 100.0 / SUM(count) OVER (), 1)::text || '%'          AS "Share"
FROM (
  SELECT
    CASE
      WHEN tu.tool_name LIKE 'mcp__ast-grep__%'
        THEN '✓  ast-grep'
      WHEN tu.tool_name = 'Bash'
        AND (tu.input->>'command') ~ 'grep.+\.(rs|ts|svelte|tsx)'
        THEN '✗  Bash grep → source files'
      WHEN tu.tool_name = 'Bash'
        AND (tu.input->>'command') ~ 'grep.+(crates/|frontend/src/)'
        AND (tu.input->>'command') !~ 'grep.+\.(toml|md|json|sh|txt|lock)'
        THEN '✗  Bash grep → source dirs'
      WHEN tu.tool_name = 'LSP' AND tu.operation IN ('workspaceSymbol', 'documentSymbol')
        THEN '✗  LSP navigation (workspaceSymbol/documentSymbol)'
      WHEN tu.tool_name = 'LSP' AND tu.operation = 'hover'
        THEN '~  LSP hover'
      WHEN tu.tool_name = 'LSP' AND tu.operation = 'findReferences'
        THEN '✓  LSP findReferences'
      WHEN tu.tool_name = 'LSP' AND tu.operation IN ('goToDefinition', 'goToImplementation')
        THEN '✓  LSP definition / impl'
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
  CASE WHEN category LIKE '✓%' THEN 0 WHEN category LIKE '~%' THEN 1 ELSE 2 END,
  count DESC;
SQL

# ── recent violations ──────────────────────────────────────────────────────────

printf "\n"

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Recent non-compliant grep commands (last 10)'
\set QUIET off

SELECT
  to_char(r.created_at, 'MM-DD HH24:MI') AS "At",
  LEFT(tu.input->>'command', 100)         AS "Command"
FROM tools.uses tu
JOIN requests.requests r ON tu.request_id = r.id
JOIN projects.projects p ON r.project_id = p.id
WHERE p.project_path = :'project'
  AND r.created_at > NOW() - (:'since')::interval
  AND tu.tool_name = 'Bash'
  AND (
    (tu.input->>'command') ~ 'grep.+\.(rs|ts|svelte|tsx)'
    OR (
      (tu.input->>'command') ~ 'grep.+(crates/|frontend/src/)'
      AND (tu.input->>'command') !~ 'grep.+\.(toml|md|json|sh|txt|lock)'
    )
  )
ORDER BY r.created_at DESC
LIMIT 10;
SQL

psql "$DB_URL" -v project="$PROJECT" -v since="$SINCE" <<'SQL'
\set QUIET on
\pset border 2
\pset linestyle unicode
\pset null '—'
\pset footer off
\pset title 'Recent LSP navigation that should be ast-grep (last 10)'
\set QUIET off

SELECT
  to_char(r.created_at, 'MM-DD HH24:MI') AS "At",
  tu.operation                            AS "Operation",
  LEFT(tu.input->>'query', 80)            AS "Query"
FROM tools.uses tu
JOIN requests.requests r ON tu.request_id = r.id
JOIN projects.projects p ON r.project_id = p.id
WHERE p.project_path = :'project'
  AND r.created_at > NOW() - (:'since')::interval
  AND tu.tool_name = 'LSP'
  AND tu.operation IN ('workspaceSymbol', 'documentSymbol')
ORDER BY r.created_at DESC
LIMIT 10;
SQL
