#!/usr/bin/env bash
# start-and-monitor.sh — build mike, run it in the background,
# then attach the real-time memory/CPU monitor.
#
# Usage:
#   ./scripts/start-and-monitor.sh              # debug build
#   ./scripts/start-and-monitor.sh --release    # optimised build
#   KILL_GiB=3.0 ./scripts/start-and-monitor.sh # custom kill threshold
#   KILL_GiB=0   ./scripts/start-and-monitor.sh # monitor only, no kill
#   INTERVAL=1   ./scripts/start-and-monitor.sh # 1-second poll

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

RELEASE_FLAG=""
BUILD_PROFILE="debug"
if [[ "${1:-}" == "--release" ]]; then
    RELEASE_FLAG="--release"
    BUILD_PROFILE="release"
fi

# ── 1. build ──────────────────────────────────────────────────────────────────
echo "==> Building mike ${RELEASE_FLAG:+(release)}"
cargo build ${RELEASE_FLAG} --manifest-path crates/mike/Cargo.toml

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -1)"
[[ -z "${TARGET_DIR}" ]] && TARGET_DIR="${PROJECT_ROOT}/target"
MIKE_BIN="${TARGET_DIR}/${BUILD_PROFILE}/mike"

# ── 2. resolve config so we know which ports to watch ─────────────────────────
eval "$("${MIKE_BIN}" print-config --shell)"
INSPECT_ADDR="${MIKE_INSPECT_ADDR}"
MIKE_PORT="${MIKE_LISTEN_ADDR##*:}"
INSPECT_PORT="${INSPECT_ADDR##*:}"

# ── 3. free stale processes on our ports ──────────────────────────────────────
free_port() {
    local port="$1"
    local pid
    pid="$(lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -z "${pid}" ]] && return 0
    local cmd; cmd="$(ps -p "${pid}" -o comm= 2>/dev/null | tr -d ' ' || echo unknown)"
    echo "==> Port ${port} held by ${cmd} (pid ${pid}) — stopping it"
    kill "${pid}" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
        sleep 0.2
        kill -0 "${pid}" 2>/dev/null || return 0
    done
    kill -9 "${pid}" 2>/dev/null || true
}

echo "==> Pre-flight: freeing ports ${MIKE_PORT}, ${INSPECT_PORT}"
free_port "${MIKE_PORT}"
free_port "${INSPECT_PORT}"

# ── 4. start proxy in background, log to file ─────────────────────────────────
LOG_FILE="/tmp/mike-$(date +%Y%m%d-%H%M%S).log"
echo "==> Starting mike (proxy :${MIKE_PORT}, inspect :${INSPECT_PORT})"
echo "==> Proxy log: ${LOG_FILE}"
"${MIKE_BIN}" >"${LOG_FILE}" 2>&1 &
MIKE_PID=$!

cleanup() {
    local code=$?
    echo
    echo "==> Shutting down proxy (PID ${MIKE_PID})"
    kill "${MIKE_PID}" 2>/dev/null || true
    wait "${MIKE_PID}" 2>/dev/null || true
    exit "${code}"
}
trap cleanup EXIT INT TERM

# ── 5. wait for inspect bind ──────────────────────────────────────────────────
echo "==> Waiting for inspect bind at http://${INSPECT_ADDR}/api/health"
WAIT_MAX=60
waited=0
while :; do
    if ! kill -0 "${MIKE_PID}" 2>/dev/null; then
        echo "==> Proxy exited unexpectedly. Last log lines:" >&2
        tail -20 "${LOG_FILE}" >&2
        exit 1
    fi
    if curl --silent --output /dev/null --fail --max-time 1 \
        "http://${INSPECT_ADDR}/api/health" 2>/dev/null; then
        break
    fi
    sleep 0.5
    waited=$(( waited + 1 ))
    if [[ "${waited}" -ge $(( WAIT_MAX * 2 )) ]]; then
        echo "==> Inspect bind not ready after ${WAIT_MAX}s" >&2
        tail -20 "${LOG_FILE}" >&2
        exit 1
    fi
done
echo "==> Proxy is up (PID ${MIKE_PID})"
echo "    Use with: ANTHROPIC_BASE_URL=http://${MIKE_LISTEN_ADDR} claude"
echo

# ── 6. attach monitor (foreground — Ctrl-C stops both) ───────────────────────
exec "${SCRIPT_DIR}/proxy-monitor.sh"
