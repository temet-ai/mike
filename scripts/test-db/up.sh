#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${MIKE_TEST_DB_PORT:-6667}"

if [ "$(uname -s)" = "Darwin" ] && [ -z "${MIKE_TEST_DB_DATA:-}" ]; then
    export MIKE_TEST_DB_DATA=mike_test_db_vm
fi

docker compose -f "${SCRIPT_DIR}/docker-compose.yml" up -d
for _ in $(seq 1 60); do
    if docker exec mike-test-pg pg_isready -U postgres -d postgres >/dev/null 2>&1; then
        echo "test db ready: postgres://postgres:postgres@127.0.0.1:${PORT}/postgres"
        exit 0
    fi
    sleep 1
done
echo "test db did not become healthy within 60s" >&2
exit 1
