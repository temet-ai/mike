#!/usr/bin/env bash
# Launch the stack with `mike` in the FOREGROUND, built with full debug info.
#
# The difference from start-proxy.sh: that one deploys mike detached and
# optimizes for not interrupting live traffic. This one puts mike in your
# terminal under the `debugging` cargo profile (full DWARF — the dev profile
# carries line tables only, so locals read as unavailable), so you can attach a
# debugger to it or just watch it work. Ctrl-C stops mike; the sidecar and SPA
# keep running.
#
# The supporting cast comes from scripts/debug-stack-deps.sh — the same
# before-launch step the RustRover "Debug mike" configuration uses:
#   - :9988 / :9989 freed for this process
#   - proxy-forward on :9987 started if down (this is what `cc` dials — the
#     stack is unreachable without it, however healthy :9988 is)
#   - Vite dev SPA on :9990 started if down
# A running sidecar or SPA is left alive, so re-running this never drops them.
#
# To attach a debugger: run this, then in RustRover Run -> Attach to Process
# and pick the `mike` PID printed below. Or set MIKE_LISTEN_ADDR /
# MIKE_INSPECT_ADDR to run a second instance alongside the deployed one.
#
# Usage:
#   ./scripts/start-debug.sh
#   MIKE_LISTEN_ADDR=127.0.0.1:19988 MIKE_INSPECT_ADDR=127.0.0.1:19989 \
#       ./scripts/start-debug.sh      # alt ports, leaves the live stack alone

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# shellcheck source=lib/managed-service.sh
source "${SCRIPT_DIR}/lib/managed-service.sh"
refuse_while_service_runs "start-debug.sh"

if [[ -f "${PROJECT_ROOT}/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "${PROJECT_ROOT}/.env"
    set +a
fi

if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

export MIKE_LISTEN_ADDR="${MIKE_LISTEN_ADDR:-127.0.0.1:9988}"
export MIKE_INSPECT_ADDR="${MIKE_INSPECT_ADDR:-127.0.0.1:9989}"
export MIKE_PERSIST_ENABLED="${MIKE_PERSIST_ENABLED:-1}"
# vite owns :9990 in dev — mike's own SPA bind would fight it for the port.
export MIKE_SPA_ENABLED=0
export RUST_LOG="${RUST_LOG:-mike=debug,tower_http=info,info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

# debug-stack-deps.sh frees these two and leaves everything else alone.
MIKE_PORT="${MIKE_LISTEN_ADDR##*:}" \
INSPECT_PORT="${MIKE_INSPECT_ADDR##*:}" \
    "${SCRIPT_DIR}/debug-stack-deps.sh"

echo
echo "==> Building mike (debugging profile — full debug info)"
MIKE_BUILD_IDENTITY=git cargo build --profile debugging --bin mike

TARGET_DIR="${CARGO_TARGET_DIR:-${PROJECT_ROOT}/target}"
MIKE_BIN="${TARGET_DIR}/debugging/mike"

cat <<EOF

  Proxy      http://${MIKE_LISTEN_ADDR}          (ANTHROPIC_BASE_URL, or via the :9987 sidecar)
  Inspect    http://${MIKE_INSPECT_ADDR}/api/requests
  Web UI     http://127.0.0.1:${MIKE_FRONTEND_PORT:-9990}/
  Binary     ${MIKE_BIN}
  PID        $$   (exec keeps this PID — RustRover -> Run -> Attach to Process)

  Ctrl-C stops mike. The sidecar and SPA keep running.

EOF

exec "${MIKE_BIN}"
