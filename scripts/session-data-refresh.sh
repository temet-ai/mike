#!/usr/bin/env bash
# session-data-refresh.sh — (re)build the golden template DB from REAL captured
# data and stamp its provenance, so per-session clones start from truth.
#
# "True data" for a capture proxy is real captured traffic restored with full
# referential integrity — not hand-authored fixtures that omit the edge cases
# that break integration. This builds the golden template that edge_provision_data
# clones per session, and records meta.golden_provenance so session-up.sh can
# enforce a freshness budget (stale data is not true data).
#
# Source of truth, pick one:
#   --from <pg_dump_file>     restore a pg_dump/pg_restore archive (sanitized export)
#   --source-url <pg_url>     pg_dump a live capture DB directly (e.g. local :6666)
#
# Usage:
#   scripts/session-data-refresh.sh --template golden_capture --from capture.dump
#   scripts/session-data-refresh.sh --template golden_capture --source-url \
#       postgres://postgres:postgres@127.0.0.1:6666/mike
#
# Admin connection: MIKE_TEST_ADMIN_URL (must point at .../postgres). The
# template is (re)created there; existing connections to it are terminated first.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
if [[ -f "${PROJECT_ROOT}/.env" ]]; then
  set -a; # shellcheck disable=SC1091
  source "${PROJECT_ROOT}/.env"; set +a
fi

TEMPLATE="golden_capture"
FROM_DUMP=""
SOURCE_URL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --template)   TEMPLATE="$2"; shift 2 ;;
    --from)       FROM_DUMP="$2"; shift 2 ;;
    --source-url) SOURCE_URL="$2"; shift 2 ;;
    *) echo "session-data-refresh.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

ADMIN="${MIKE_TEST_ADMIN_URL:-}"
[[ -n "${ADMIN}" ]] || { echo "✗ MIKE_TEST_ADMIN_URL unset (point it at .../postgres)" >&2; exit 1; }
command -v psql >/dev/null 2>&1 || { echo "✗ psql not on PATH" >&2; exit 1; }
[[ -n "${FROM_DUMP}" || -n "${SOURCE_URL}" ]] || {
  echo "✗ provide --from <dump> or --source-url <pg_url>" >&2; exit 2;
}

TEMPLATE_URL="${ADMIN%/*}/${TEMPLATE}"
SOURCE_LABEL=""

echo "==> (re)creating golden template '${TEMPLATE}'"
# Terminate stragglers, then drop+create so the restore lands in a clean DB.
psql "${ADMIN}" -v ON_ERROR_STOP=1 -qc \
  "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${TEMPLATE}' AND pid <> pg_backend_pid()" >/dev/null
psql "${ADMIN}" -v ON_ERROR_STOP=1 -qc "DROP DATABASE IF EXISTS \"${TEMPLATE}\"" >/dev/null
psql "${ADMIN}" -v ON_ERROR_STOP=1 -qc "CREATE DATABASE \"${TEMPLATE}\"" >/dev/null

echo "==> ensuring extensions"
psql "${TEMPLATE_URL}" -v ON_ERROR_STOP=1 -qc \
  "CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS pg_trgm;" >/dev/null

if [[ -n "${SOURCE_URL}" ]]; then
  command -v pg_dump >/dev/null 2>&1 || { echo "✗ pg_dump not on PATH" >&2; exit 1; }
  echo "==> dumping live source → restoring into '${TEMPLATE}'"
  pg_dump --no-owner --no-privileges -Fc "${SOURCE_URL}" \
    | pg_restore --no-owner --no-privileges --exit-on-error -d "${TEMPLATE_URL}"
  SOURCE_LABEL="source-url:${SOURCE_URL%%@*}@…"
else
  echo "==> restoring '${FROM_DUMP}' into '${TEMPLATE}'"
  [[ -f "${FROM_DUMP}" ]] || { echo "✗ dump not found: ${FROM_DUMP}" >&2; exit 1; }
  command -v pg_restore >/dev/null 2>&1 || { echo "✗ pg_restore not on PATH" >&2; exit 1; }
  pg_restore --no-owner --no-privileges --exit-on-error -d "${TEMPLATE_URL}" "${FROM_DUMP}"
  SOURCE_LABEL="dump:$(basename "${FROM_DUMP}")"
fi

# Stamp provenance so the freshness budget has something true to read. The
# schema_hash ties the data snapshot to the migration set it was captured under,
# so a session can see data/schema skew, not just age.
SCHEMA_HASH="$(git -C "${PROJECT_ROOT}" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "==> stamping meta.golden_provenance"
psql "${TEMPLATE_URL}" -v ON_ERROR_STOP=1 -qc "
CREATE SCHEMA IF NOT EXISTS meta;
CREATE TABLE IF NOT EXISTS meta.golden_provenance (
  captured_at timestamptz NOT NULL DEFAULT now(),
  source      text        NOT NULL,
  schema_hash text        NOT NULL
);
INSERT INTO meta.golden_provenance (source, schema_hash)
VALUES ('${SOURCE_LABEL}', '${SCHEMA_HASH}');" >/dev/null

echo "==> golden template '${TEMPLATE}' rebuilt (source=${SOURCE_LABEL}, schema=${SCHEMA_HASH})."
echo "    Per-session clones now start from this snapshot. Re-run to refresh."
