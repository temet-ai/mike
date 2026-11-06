#!/usr/bin/env bash
# session-up.sh — bring up an isolated dev session with ACCURATE EDGES, or fail.
#
# An isolated session (worktree or container) only saves work if it starts from
# truth: the interfaces it builds against and the data it serves must be current
# against the real producer at boot. Otherwise the isolated piece "thinks" it's
# right and breaks at integration — the rework that makes isolation expensive.
#
# This is the boot gate. It refuses to hand over the session until the edges its
# profile declares are provably current (fail-closed), mirroring the start-proxy
# DB probe and preflight-toolchain self-heal. It does NOT replace start-proxy —
# it runs the edge preflight, prints the resolved session environment, and (for
# service profiles) tells you the exact next command.
#
# Usage:
#   scripts/session-up.sh <profile> [--max-data-age H] [--no-clone]
#   scripts/session-up.sh reconcile        # re-stamp interface baselines
#   scripts/session-up.sh profiles         # list profiles + their edges
#
# Profiles (each materializes only the edges it consumes — like the MCP exposing
# a declared subset of the inspect API, not a full mirror):
#   frontend  — SPA work. Needs current API types + a real seed; NO cargo rebuild.
#   backend   — Rust logic. Needs a current .sqlx cache; builds DB-less offline.
#   db        — backend + e2e. Needs .sqlx + a per-session clone of real data.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# Load .env (MIKE_TEST_ADMIN_URL, MIKE_DATABASE_URL, …) the way start-proxy does.
if [[ -f "${PROJECT_ROOT}/.env" ]]; then
  set -a; # shellcheck disable=SC1091
  source "${PROJECT_ROOT}/.env"; set +a
fi

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/preflight-edge.sh"

# ── Per-profile edge manifest ────────────────────────────────────────────────
# Each profile lists the edges it must materialize at boot. Tokens:
#   iface:<name>            hash-pinned derived interface (sqlx | api-types)
#   data:<template>@<hours> golden template cloned per session, max age <hours>
#   svc:<name>              real dependency to bring up AFTER preflight passes
# A profile carries only the edges it actually consumes; absence is not a gap.
declare -A PROFILE_EDGES=(
  [frontend]="iface:api-types data:golden_capture@24 svc:inspect svc:spa"
  [backend]="iface:sqlx svc:build"
  [db]="iface:sqlx data:golden_capture@24 svc:proxy svc:inspect"
)

# Default freshness budget (hours) if a profile's data token omits @H.
DEFAULT_MAX_DATA_AGE=24

list_profiles() {
  echo "Profiles and the edges each boots:"
  local p
  for p in "${!PROFILE_EDGES[@]}"; do
    printf '  %-9s %s\n' "${p}" "${PROFILE_EDGES[$p]}"
  done | sort
}

reconcile_all() {
  echo "Re-stamping interface baselines from current producer state:"
  local seen=" " name
  for p in "${!PROFILE_EDGES[@]}"; do
    for tok in ${PROFILE_EDGES[$p]}; do
      [[ "${tok}" == iface:* ]] || continue
      name="${tok#iface:}"
      [[ "${seen}" == *" ${name} "* ]] && continue
      seen+="${name} "
      edge_reconcile_interface "${name}"
    done
  done
  echo "Done. Commit scripts/.edge/*.hash so every session shares the baseline."
}

# ── Argument parsing ─────────────────────────────────────────────────────────
[[ $# -ge 1 ]] || { echo "usage: session-up.sh <profile|reconcile|profiles> [--max-data-age H] [--no-clone]" >&2; exit 2; }

case "$1" in
  profiles)  list_profiles; exit 0 ;;
  reconcile) reconcile_all; exit 0 ;;
esac

PROFILE="$1"; shift
MAX_AGE_OVERRIDE=""
DO_CLONE=true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --max-data-age) MAX_AGE_OVERRIDE="$2"; shift 2 ;;
    --no-clone)     DO_CLONE=false; shift ;;
    *) echo "session-up.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

[[ -n "${PROFILE_EDGES[$PROFILE]:-}" ]] || {
  echo "✗ unknown profile '${PROFILE}'." >&2; list_profiles >&2; exit 2;
}

echo "==> session-up: profile '${PROFILE}' — edge-fidelity preflight (fail-closed)"

# ── 1. Toolchain preflight (reuse the existing self-heal), build profiles only ─
if [[ "${PROFILE_EDGES[$PROFILE]}" == *"svc:build"* \
   || "${PROFILE_EDGES[$PROFILE]}" == *"svc:proxy"* \
   || "${PROFILE_EDGES[$PROFILE]}" == *"iface:sqlx"* ]]; then
  # shellcheck disable=SC1091
  source "${SCRIPT_DIR}/lib/preflight-toolchain.sh"
  ensure_build_toolchain
fi

# ── 2. Interface edges — hash-pinned to producer, fail closed on drift ─────────
SESSION_DB_URL=""
FAILED=0
for tok in ${PROFILE_EDGES[$PROFILE]}; do
  case "${tok}" in
    iface:*)
      edge_check_interface "${tok#iface:}" || FAILED=1
      ;;
    data:*)
      ${DO_CLONE} || { echo "  • edge[data]: skipped (--no-clone)"; continue; }
      spec="${tok#data:}"
      template="${spec%@*}"
      age="${MAX_AGE_OVERRIDE:-}"
      [[ -z "${age}" && "${spec}" == *@* ]] && age="${spec#*@}"
      [[ -z "${age}" ]] && age="${DEFAULT_MAX_DATA_AGE}"
      admin="${MIKE_TEST_ADMIN_URL:-}"
      if [[ -z "${admin}" ]]; then
        echo "  ✗ edge[data]: MIKE_TEST_ADMIN_URL unset (must point at .../postgres)" >&2
        FAILED=1; continue
      fi
      if url="$(edge_provision_data "${template}" "${age}" "${admin}")"; then
        SESSION_DB_URL="${url}"
      else
        FAILED=1
      fi
      ;;
    svc:*) : ;;  # handled after preflight passes
  esac
done

if [[ "${FAILED}" -ne 0 ]]; then
  echo "✗ session-up: edges not current — refusing to start (fail-closed)." >&2
  echo "  Make the edges true (see fixes above), then re-run. Isolation only saves" >&2
  echo "  work when the session starts from truth." >&2
  exit 1
fi

# ── 3. Hand over — write a sourceable env file + print the next command ───────
# The env file is how the resolved per-session environment (notably the DB clone
# URL) reaches the workload: `source .session-env` in a worktree, or the
# container entrypoint sources it before exec. Path overridable via
# MIKE_SESSION_ENV. Gitignored — it is per-session scratch.
SESSION_ENV_FILE="${MIKE_SESSION_ENV:-${PROJECT_ROOT}/.session-env}"
{
  echo "# generated by session-up.sh for profile '${PROFILE}' — source me"
  echo "export MIKE_SESSION_PROFILE='${PROFILE}'"
  [[ -n "${SESSION_DB_URL}" ]] && echo "export MIKE_DATABASE_URL='${SESSION_DB_URL}'"
} > "${SESSION_ENV_FILE}"

echo "==> edges current. Wrote ${SESSION_ENV_FILE#"${PROJECT_ROOT}/"} (source it). Profile '${PROFILE}':"
if [[ -n "${SESSION_DB_URL}" ]]; then
  echo "    MIKE_DATABASE_URL=${SESSION_DB_URL}   # per-session clone of real data"
fi
case "${PROFILE}" in
  frontend)
    echo "    # no cargo rebuild; talk to a shared/real inspect API:"
    echo "    scripts/restart-frontend.sh        # SPA on :9990 (MIKE_FRONTEND_PORT to move it)"
    ;;
  backend)
    echo "    # builds DB-less against the committed .sqlx offline cache:"
    echo "    scripts/cx check -p mike   # or: make test-p p=mike"
    ;;
  db)
    echo "    # real proxy + inspect against the per-session DB clone above:"
    echo "    ./scripts/start-proxy.sh"
    ;;
esac
echo "==> session-up: ready."
