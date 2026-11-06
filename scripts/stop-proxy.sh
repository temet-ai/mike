#!/usr/bin/env bash
# stop-proxy.sh — gracefully stop the full mike stack.
#
# Stops the systemd user units that run the stack (mike-stack-*: the proxy, the
# sidecar, the prompt assistant, the web UI, Storybook and the architecture
# diagrams), so systemd does not bring them back, then frees the known ports as
# a belt-and-suspenders cleanup for anything running outside a unit.
#
# Usage:
#   ./scripts/stop-proxy.sh            # default ports (9988, 9989, 9990, 6006, 9987, 9985, 9986)
#   STORYBOOK_PORT=6007 ./scripts/stop-proxy.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── port config (mirrors start-proxy.sh) ─────────────────────────────────────
# Prefer the binary's resolved config; fall back to defaults so stop-proxy
# works even when the proxy binary is not on PATH or hasn't been built yet.
MIKE_PORT=9988
INSPECT_PORT=9989
SPA_PORT=9990
STORYBOOK_PORT="${MIKE_STORYBOOK_PORT:-6006}"
MIKE_FORWARD_PORT="${MIKE_FORWARD_PORT:-9987}"   # HTTPS_PROXY MITM sidecar
PROMPT_ASSISTANT_PORT="${PROMPT_ASSISTANT_PORT:-9985}"   # Claude Agent SDK service (services/prompt-assistant)
ARCH_PORT="${ARCH_PORT:-9986}"   # architecture diagrams (scripts/arch serve)

MIKE_BIN=""
if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

# Try to read effective ports from the binary so a custom config is honoured.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --offline \
    --manifest-path "${PROJECT_ROOT}/crates/mike/Cargo.toml" 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -1 || true)"
[[ -z "${TARGET_DIR}" ]] && TARGET_DIR="${PROJECT_ROOT}/target"

for profile in debug release; do
    bin="${TARGET_DIR}/${profile}/mike"
    if [[ -x "${bin}" ]]; then
        MIKE_BIN="${bin}"
        break
    fi
done

if [[ -n "${MIKE_BIN}" ]]; then
    if config_out="$("${MIKE_BIN}" print-config --shell 2>/dev/null)"; then
        eval "${config_out}"
        MIKE_PORT="${MIKE_LISTEN_ADDR##*:}"
        INSPECT_PORT="${MIKE_INSPECT_ADDR##*:}"
        SPA_PORT="${MIKE_FRONTEND_PORT:-${SPA_PORT}}"
    fi
fi

PORTS=("${MIKE_PORT}" "${INSPECT_PORT}" "${SPA_PORT}" "${STORYBOOK_PORT}" "${MIKE_FORWARD_PORT}" "${PROMPT_ASSISTANT_PORT}" "${ARCH_PORT}")

# ── helpers ───────────────────────────────────────────────────────────────────
stop_port() {
    local port="$1"
    local pid
    pid="$(lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -z "${pid}" ]] && return 0
    local cmd
    cmd="$(ps -p "${pid}" -o comm= 2>/dev/null | tr -d ' ' || echo unknown)"
    echo "==> Stopping ${cmd} (pid ${pid}) on port ${port}"
    kill "${pid}" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        sleep 0.3
        kill -0 "${pid}" 2>/dev/null || return 0
    done
    echo "==> pid ${pid} did not exit on SIGTERM — sending SIGKILL"
    kill -9 "${pid}" 2>/dev/null || true
}

# ── main ──────────────────────────────────────────────────────────────────────
echo "==> Stopping mike stack (ports ${PORTS[*]})"

if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
    for unit in mike-stack-storybook mike-stack-arch mike-stack-spa mike-stack-assistant mike-stack-forward mike-stack-proxy; do
        if systemctl --user is-active --quiet "${unit}.service"; then
            echo "==> Stopping ${unit} (systemd will not restart it until the next start or deploy)"
            systemctl --user stop "${unit}.service" || true
        fi
    done
fi

for port in "${PORTS[@]}"; do
    stop_port "${port}"
done

# Verify all ports are free.
all_free=true
for port in "${PORTS[@]}"; do
    if lsof -ti ":${port}" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "==> WARNING: port ${port} is still in use after stop"
        all_free=false
    fi
done

if ${all_free}; then
    echo "==> Stack stopped — all ports are free"
else
    echo "==> Stack partially stopped — check warnings above"
    exit 1
fi
