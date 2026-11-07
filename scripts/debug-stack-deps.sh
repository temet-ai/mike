#!/usr/bin/env bash
# Before-launch step for the RustRover "Debug mike" run configuration.
#
# The debugger starts `mike` itself, so this script's job is only the parts of
# the stack `mike` does NOT provide, plus clearing its way:
#
#   1. Free :9988 / :9989 — kill whatever proxy currently holds them (a
#      deploy-proxy.sh instance, or a previous debug run) so the debugged
#      binary can bind.
#   2. proxy-forward on :9987 — the sidecar `cc` points HTTPS_PROXY at. Without
#      it a new Claude Code session gets connection-refused no matter how
#      healthy :9988 is. Started only when down; a live one is left alone.
#   3. Vite dev SPA on :9990 — the web UI. Started only when down; HMR means a
#      running one never needs a restart.
#
# Deliberately NOT stop-proxy.sh: that also kills :9987, :9990 and :6006, which
# is what leaves a debug session with a working proxy and no way to reach it.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# shellcheck source=lib/managed-service.sh
source "${SCRIPT_DIR}/lib/managed-service.sh"
refuse_while_service_runs "debug-stack-deps.sh"

if [[ -f "${PROJECT_ROOT}/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "${PROJECT_ROOT}/.env"
    set +a
fi

if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

MIKE_PORT="${MIKE_PORT:-9988}"
INSPECT_PORT="${INSPECT_PORT:-9989}"
SPA_PORT="${MIKE_FRONTEND_PORT:-9990}"
export MIKE_FORWARD_PORT="${MIKE_FORWARD_PORT:-9987}"

# ensure_proxy_forward + free_port, shared with the deploy scripts. It reads
# these four at call time.
export TARGET_DIR="${CARGO_TARGET_DIR:-${PROJECT_ROOT}/target}"
export BUILD_PROFILE="debug"
export RELEASE_FLAG=""
export FORCE="false"
# shellcheck source=lib/proxy-runtime.sh
source "${SCRIPT_DIR}/lib/proxy-runtime.sh"

# ── 1. Clear the ports the debugged binary needs ─────────────────────────────
for port in "${MIKE_PORT}" "${INSPECT_PORT}"; do
    if lsof -ti ":${port}" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "==> Freeing :${port} for the debug session"
        free_port "${port}"
    fi
done

# ── 2. Sidecar — the address `cc` actually dials ─────────────────────────────
ensure_proxy_forward

# ── 3. Web UI ────────────────────────────────────────────────────────────────
if lsof -ti ":${SPA_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "==> SPA already running on :${SPA_PORT} — leaving it (HMR picks up changes live)"
else
    mkdir -p "${PROJECT_ROOT}/.deploy"
    if [[ ! -d "${PROJECT_ROOT}/frontend/node_modules" ]]; then
        echo "==> Installing frontend dependencies (first run)"
        (cd "${PROJECT_ROOT}/frontend" && pnpm install)
    fi
    echo "==> Starting SPA on :${SPA_PORT} (detached; log -> .deploy/spa.log)"
    (
        cd "${PROJECT_ROOT}/frontend"
        INSPECT_BACKEND="http://127.0.0.1:${INSPECT_PORT}" \
        MIKE_FRONTEND_PORT="${SPA_PORT}" \
            nohup pnpm run dev > "${PROJECT_ROOT}/.deploy/spa.log" 2>&1 &
        disown $! 2>/dev/null || true
    )
fi

echo "==> Stack deps ready — starting the debugged proxy on :${MIKE_PORT} / :${INSPECT_PORT}"
