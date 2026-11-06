#!/usr/bin/env bash
# Start (or resume) the shared capture DB container and wait until it answers.
#
#   scripts/capture-db/up.sh
#
# Idempotent: re-running against a healthy container is a no-op. Never touches
# the data — the volume is external (see docker-compose.yml), and this script
# only creates it when it does not exist yet (a fresh machine), which yields an
# empty cluster that initdb populates; migrations then run on proxy startup.
#
# Env overrides: CAPTURE_DB_PORT (5440), CAPTURE_DB_USER (devuser),
# CAPTURE_DB_PASSWORD, CAPTURE_DB_WAIT_SECS (300 — crash recovery on a large
# cluster after an unclean stop can take a while).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE=("docker" "compose" "-f" "${SCRIPT_DIR}/docker-compose.yml")
CONTAINER="armitage-pg"
[[ -f "${SCRIPT_DIR}/.env" ]] && { set -a; source "${SCRIPT_DIR}/.env"; set +a; }
VOLUME="${CAPTURE_DB_VOLUME:-armitage_pg_data}"
PORT="${CAPTURE_DB_PORT:-5440}"
USER_NAME="${CAPTURE_DB_USER:-devuser}"
WAIT_SECS="${CAPTURE_DB_WAIT_SECS:-300}"

docker info >/dev/null 2>&1 || { echo "capture-db: docker daemon is not reachable" >&2; exit 1; }

if ! docker volume inspect "${VOLUME}" >/dev/null 2>&1; then
    echo "==> volume ${VOLUME} does not exist; creating an EMPTY one (fresh cluster)"
    docker volume create "${VOLUME}" >/dev/null
fi

echo "==> starting ${CONTAINER} (PostgreSQL 19 + pgvector, host port ${PORT})"
"${COMPOSE[@]}" up -d

echo "==> waiting up to ${WAIT_SECS}s for health"
deadline=$(( $(date +%s) + WAIT_SECS ))
status="unknown"
while (( $(date +%s) < deadline )); do
    status="$(docker inspect --format '{{.State.Health.Status}}' "${CONTAINER}" 2>/dev/null || echo missing)"
    [[ "${status}" == "healthy" ]] && break
    if [[ "${status}" == "missing" ]] || [[ "$(docker inspect --format '{{.State.Status}}' "${CONTAINER}" 2>/dev/null)" == "exited" ]]; then
        echo "capture-db: ${CONTAINER} is not running; last log lines:" >&2
        docker logs --tail 40 "${CONTAINER}" >&2 || true
        exit 1
    fi
    sleep 2
done
if [[ "${status}" != "healthy" ]]; then
    echo "capture-db: ${CONTAINER} still '${status}' after ${WAIT_SECS}s; last log lines:" >&2
    docker logs --tail 40 "${CONTAINER}" >&2 || true
    exit 1
fi

preload="$(docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d postgres -tAc "SHOW shared_preload_libraries")"
if [[ "${preload}" != *pg_stat_statements* ]]; then
    echo "==> enabling pg_stat_statements (takes effect after one container restart)"
    docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d postgres -qc \
        "ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements'; ALTER SYSTEM SET pg_stat_statements.track = 'all'; ALTER SYSTEM SET pg_stat_statements.max = '10000'; ALTER SYSTEM SET track_io_timing = 'on'; ALTER SYSTEM SET log_lock_waits = 'on'"
    docker restart "${CONTAINER}" >/dev/null
    until [[ "$(docker inspect --format '{{.State.Health.Status}}' "${CONTAINER}")" == "healthy" ]]; do sleep 2; done
fi
if docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'claude_proxy'" | grep -q 1; then
    docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d claude_proxy -qc "CREATE EXTENSION IF NOT EXISTS pg_stat_statements"
fi

dbs="$(docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d postgres -tAc \
    "SELECT string_agg(datname, ', ' ORDER BY datname) FROM pg_database WHERE NOT datistemplate")"
version="$(docker exec "${CONTAINER}" psql -U "${USER_NAME}" -d postgres -tAc "SHOW server_version")"
echo "==> ${CONTAINER} healthy — PostgreSQL ${version}; databases: ${dbs}"
echo "==> capture DB ready. .env should carry:"
echo "    MIKE_DATABASE_URL=postgres://${USER_NAME}:<password>@127.0.0.1:${PORT}/claude_proxy"
