#!/usr/bin/env bash
# Stop the shared capture DB container. The data volume is external and is
# never removed by this script — there is deliberately no --destroy here; the
# cluster is the capture history of every session and deleting it is a manual,
# operator-only act (docker volume rm armitage_pg_data).
#
#   scripts/capture-db/down.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
docker compose -f "${SCRIPT_DIR}/docker-compose.yml" down
echo "==> capture DB stopped (data kept in volume armitage_pg_data)"
