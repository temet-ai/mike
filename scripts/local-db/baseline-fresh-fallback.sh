#!/usr/bin/env bash
# Explicit development fallback-only seed cursor initialization. Never invoked by
# installed provisioning or primary setup. Run only after embedded migrations have
# completed on a fresh fallback and before its first capture or resync.
set -euo pipefail
if [[ "$#" -ne 1 || "$1" != --confirmed-fresh-fallback ]]; then
  echo 'Usage: MIKE_LOCAL_DATABASE_URL=... baseline-fresh-fallback.sh --confirmed-fresh-fallback' >&2
  echo 'Only use on a fresh fallback, after mike serve migrated it and before capture/resync.' >&2
  exit 2
fi
: "${MIKE_LOCAL_DATABASE_URL:?Set the explicitly selected fresh fallback URL}"
PGDATABASE="$MIKE_LOCAL_DATABASE_URL" PGOPTIONS='-c statement_timeout=5000 -c lock_timeout=2000' psql -X -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO sync.cursor (table_name, last_local_id)
SELECT 'system_prompts.system_prompts', COALESCE(MAX(id), 0) FROM system_prompts.system_prompts
UNION ALL SELECT 'overrides.overrides', COALESCE(MAX(id), 0) FROM overrides.overrides
UNION ALL SELECT 'overrides.revisions', COALESCE(MAX(id), 0) FROM overrides.revisions
ON CONFLICT (table_name) DO NOTHING;
SQL
