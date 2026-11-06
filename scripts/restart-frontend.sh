#!/usr/bin/env bash
# Restart the SvelteKit SPA host only. Leaves the Rust proxy at
# :9988 / :9989 untouched — use this when your changes are scoped to
# frontend/ and you don't want to tear down the proxy or lose its state.
#
# The SPA runs `vite dev` (HMR on) by default — file edits land live in the
# browser without a rebuild, so this script is really only needed when the
# dev server itself has died (node crash, port held by another process,
# config change requiring a clean boot).
#
# Usage:
#   ./scripts/restart-frontend.sh
#
# Behaviour:
#   1. Verifies the Rust proxy at :9989 is reachable (fails fast otherwise)
#   2. Installs frontend deps if node_modules/ is missing
#   3. Under systemd (the normal case) restarts the units that own them,
#      mike-stack-spa and mike-stack-storybook, and returns. Killing their
#      processes instead would only have systemd start them again underneath a
#      second copy.
#   4. Without systemd, or on non-default ports: kills whoever is holding
#      :9990 / :6006, starts `vite dev` and `storybook dev` in the foreground,
#      and Ctrl-C cleans up the children via the EXIT trap

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

SPA_PORT="${MIKE_FRONTEND_PORT:-9990}"
STORYBOOK_PORT="${MIKE_STORYBOOK_PORT:-6006}"
INSPECT_ADDR="${MIKE_INSPECT_ADDR:-127.0.0.1:9989}"

# Kill whoever is currently bound to a given TCP port, if anything.
# Mirrors the helper in start-proxy.sh. Idempotent: no-op if the port is free.
free_port() {
    local port="$1"
    local pid
    pid="$(lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -z "${pid}" ]]; then
        return 0
    fi
    local cmd
    cmd="$(ps -p "${pid}" -o comm= 2>/dev/null | tr -d ' ' || echo unknown)"
    echo "==> Port ${port} held by ${cmd} (pid ${pid}) — stopping it"
    kill "${pid}" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
        sleep 0.2
        if ! kill -0 "${pid}" 2>/dev/null; then
            return 0
        fi
    done
    echo "==> pid ${pid} did not exit on SIGTERM; sending SIGKILL"
    kill -9 "${pid}" 2>/dev/null || true
    sleep 0.2
}

PIDS=()

cleanup() {
    local exit_code=$?
    echo
    echo "==> Stopping vite dev + storybook (Rust proxy is untouched)..."
    for pid in "${PIDS[@]}"; do
        if kill -0 "${pid}" 2>/dev/null; then
            kill "${pid}" 2>/dev/null || true
        fi
    done
    wait "${PIDS[@]}" 2>/dev/null || true
    # Belt-and-suspenders: free both ports in case the dev servers detached
    # past the subshell PIDs tracked in PIDS.
    free_port "${SPA_PORT}"
    free_port "${STORYBOOK_PORT}"
    exit "${exit_code}"
}
trap cleanup EXIT INT TERM

# Fail fast if the Rust proxy isn't actually running. Starting vite dev
# against a dead inspect bind floods the console with ECONNREFUSED errors
# (every /api/stats poll, every /api/requests fetch, every WS reconnect)
# and leaves the SPA broken with no way to diagnose it from the browser.
if ! curl --silent --output /dev/null --fail --max-time 1 \
    "http://${INSPECT_ADDR}/api/health"; then
    echo "==> Rust proxy is not reachable at http://${INSPECT_ADDR}/api/health"
    echo "    The inspect bind must be up before the SPA can talk to it."
    echo "    This script only restarts the frontend — the Rust proxy"
    echo "    needs ./scripts/start-proxy.sh, which boots the full stack."
    echo
    # Ask the user whether to take over and boot the full stack ourselves.
    # Only prompt when stdin is a real TTY — otherwise (CI, piped input,
    # background invocations) abort cleanly so callers get a deterministic
    # exit code instead of a hung read.
    if [[ -t 0 ]]; then
        read -r -p "==> Start it now via ./scripts/start-proxy.sh? [Y/n] " answer
        case "${answer}" in
            ""|y|Y|yes|YES)
                # Disarm our EXIT trap before handing off — there's nothing
                # for us to clean up (vite dev was never started), and
                # we don't want our trap to free the SPA port the new
                # process is about to take over.
                trap - EXIT INT TERM
                echo "==> Handing off to ./scripts/start-proxy.sh"
                exec "${SCRIPT_DIR}/start-proxy.sh" "$@"
                ;;
            *)
                echo "==> Aborted. Run ./scripts/start-proxy.sh when you're ready."
                exit 1
                ;;
        esac
    else
        echo "    Run ./scripts/start-proxy.sh, then rerun this script. Aborting."
        exit 1
    fi
fi
echo "==> Rust proxy is reachable at http://${INSPECT_ADDR}/api/health"

# Install frontend deps on first run. Mirrors start-proxy.sh — `vite dev`
# would otherwise crash on a missing-module Rollup error and leave the
# port half-bound.
if [[ ! -d "${PROJECT_ROOT}/frontend/node_modules" ]]; then
    echo "==> Installing frontend dependencies (first run)"
    (cd frontend && pnpm install)
fi

# The units own the dev servers when systemd is there; restart those and hand
# the terminal back. The trap is disarmed first: its port-freeing would kill
# the processes the units just started.
source "${SCRIPT_DIR}/lib/stack-units.sh"
if stack_units_enabled && [[ "${SPA_PORT}" == "9990" && "${STORYBOOK_PORT}" == "6006" ]]; then
    trap - EXIT INT TERM
    install_stack_units
    for unit in mike-stack-spa mike-stack-storybook; do
        echo "==> Restarting ${unit}"
        unit_restart "${unit}" || echo "==> ${unit} failed to restart — see systemctl --user status ${unit}" >&2
    done
    echo
    echo "==> Frontend restarted under systemd. Rust proxy at :9988/:9989 untouched."
    echo "    web UI:       http://127.0.0.1:${SPA_PORT}/      (log: .deploy/spa.log)"
    echo "    storybook:    http://127.0.0.1:${STORYBOOK_PORT}/      (log: .deploy/storybook.log; takes a few seconds to answer)"
    exit 0
fi

echo "==> Pre-flight: freeing ports ${SPA_PORT}, ${STORYBOOK_PORT}"
free_port "${SPA_PORT}"
free_port "${STORYBOOK_PORT}"

echo "==> Starting SPA host (vite dev — HMR enabled)"
echo "    web UI:       http://127.0.0.1:${SPA_PORT}/"
(cd frontend && pnpm run start) &
PIDS+=($!)

echo "==> Starting Storybook (story sandbox)"
echo "    storybook:    http://127.0.0.1:${STORYBOOK_PORT}/"
(cd frontend && ./node_modules/.bin/storybook dev -p "${STORYBOOK_PORT}" --no-open) &
PIDS+=($!)

echo
echo "==> Frontend is up. Ctrl-C to stop. Rust proxy at :9988/:9989 untouched."
echo

wait -n
