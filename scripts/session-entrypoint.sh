#!/usr/bin/env bash
# session-entrypoint.sh — container entrypoint for docker-compose.dev.yml.
#
# Runs the edge-fidelity preflight (scripts/session-up.sh <profile>) as a
# FAIL-CLOSED gate, then execs the profile's long-running command. A container
# whose interfaces or data are stale never reaches the serve step — that is the
# whole point: the isolated session starts from truth, or not at all.
#
# Usage (invoked by compose): session-entrypoint.sh <frontend|backend|db>
set -euo pipefail

PROFILE="${1:?usage: session-entrypoint.sh <frontend|backend|db>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

echo "==> [entrypoint] edge preflight for profile '${PROFILE}'"
# Fail-closed: if session-up exits non-zero, so do we — compose marks the
# container failed instead of serving on stale edges.
"${SCRIPT_DIR}/session-up.sh" "${PROFILE}"

# Pull the resolved per-session environment (e.g. MIKE_DATABASE_URL = the DB
# clone) into this shell so the workload below inherits it.
if [[ -f "${PROJECT_ROOT}/.session-env" ]]; then
  set -a; # shellcheck disable=SC1091
  source "${PROJECT_ROOT}/.session-env"; set +a
fi

echo "==> [entrypoint] edges current — starting '${PROFILE}' workload"
case "${PROFILE}" in
  frontend)
    # SPA dev server; its edge is the real inspect API at MIKE_INSPECT_URL.
    corepack pnpm -C frontend install --frozen-lockfile
    exec corepack pnpm -C frontend run dev -- --host 0.0.0.0 --port "${MIKE_FRONTEND_PORT:-9990}"
    ;;
  backend)
    # No long-running service: a DB-less logic shell. Keep the container alive
    # so an agent/operator can attach and run `scripts/cx ...`.
    echo "    backend session ready — attach with: docker compose -f docker-compose.dev.yml exec backend bash"
    exec sleep infinity
    ;;
  db)
    # session-up already cloned a per-session DB and printed MIKE_DATABASE_URL;
    # start the real proxy + inspect stack against it.
    exec ./scripts/start-proxy.sh
    ;;
  *)
    echo "✗ [entrypoint] unknown profile '${PROFILE}'" >&2
    exit 2
    ;;
esac
