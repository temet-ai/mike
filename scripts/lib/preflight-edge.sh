#!/usr/bin/env bash
# preflight-edge.sh — edge-fidelity preflight for isolated dev sessions.
#
# The problem this solves: an isolated session (worktree or container) that
# builds against a stale interface, or serves hand-authored fake data, "thinks"
# it did the work right and then breaks at real integration. The fix is to make
# accurate edges a BOOT PRECONDITION — the session is not handed over until its
# interfaces and data are provably current against the real producer, or the
# boot fails closed.
#
# Two kinds of edge, both DERIVED from the real producer, never authored beside
# it:
#
#   1. Interfaces — generated/derived artifacts (the .sqlx query cache, the
#      frontend API types) hash-pinned to their producer source. If the producer
#      moved since the artifact was last reconciled, the artifact is stale and
#      boot fails. This is the same discipline as the committed .sqlx cache, made
#      a boot check.
#
#   2. Data — a golden template DB cloned per session, refused if the template is
#      older than the profile's freshness budget. "True data" is a real captured
#      snapshot restored with referential integrity, not synthesized fixtures.
#
# Source this file and call the edge_* functions, or let scripts/session-up.sh
# drive it. Modelled on scripts/lib/preflight-toolchain.sh (sourceable, fast,
# fail-closed) and the start-proxy.sh DB probe (pure-bash parse + clear exit).
#
# Reconcile model: scripts/.edge/<name>.hash records the producer state at which
# the interface artifact was last known-good. After you intentionally regenerate
# an artifact (cargo sqlx prepare, the TS type generator), run
# `scripts/session-up.sh reconcile` to re-stamp these hashes. A producer change
# WITHOUT a matching reconcile is exactly the drift this preflight catches.

# Resolve repo root once; every function is cwd-independent.
_EDGE_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EDGE_REPO_ROOT="$(cd "${_EDGE_LIB_DIR}/../.." && pwd)"
source "${_EDGE_LIB_DIR}/sha256.sh"
EDGE_HASH_DIR="${EDGE_REPO_ROOT}/scripts/.edge"

# The committed derived ARTIFACT each interface pins, by name. Two layers, by
# design (see docs/SESSION_CONTAINERS.md):
#   - boot (here, fast): hash the committed artifact and refuse to start if it
#     was changed without a matching reconcile — catches "regenerated but didn't
#     reconcile" and hand-edits of generated output. No cargo build, no DB, so it
#     stays a sub-second boot gate and does not false-flag on unrelated source
#     edits (e.g. a non-DTO change in the 10k-line handlers.rs).
#   - CI (scripts/check-generated-types.sh, `cargo sqlx prepare --check`):
#     regenerate from the producer and diff — the authoritative "artifact matches
#     the Rust source" check. It needs a build, so it does not run at boot.
# This mirrors how the .sqlx offline cache already works: committed artifact,
# `prepare --check` in CI.
_edge_artifact_path() {
  case "$1" in
    sqlx)      echo "${EDGE_REPO_ROOT}/.sqlx" ;;
    api-types) echo "${EDGE_REPO_ROOT}/frontend/src/lib/api/generated" ;;
    *)         return 1 ;;
  esac
}

# How to regenerate the artifact (printed when stale, so the fix is one copy-paste).
_edge_regen_hint() {
  case "$1" in
    sqlx)      echo 'cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL"' ;;
    api-types) echo 'cargo test -p mike --features ts-export export_bindings' ;;
    *)         echo 'regenerate the derived artifact, then commit' ;;
  esac
}

# Content hash of an interface's committed artifact: hash of every tracked file
# under its path, path-sorted for determinism. Uses git when available (fast,
# respects .gitignore), else find.
_edge_artifact_hash() {
  local name="$1" paths
  paths="$(_edge_artifact_path "${name}")" || return 1
  local files=""
  if git -C "${EDGE_REPO_ROOT}" rev-parse --git-dir >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    files="$(cd "${EDGE_REPO_ROOT}" && git ls-files -- ${paths} 2>/dev/null)"
  fi
  if [[ -z "${files}" ]]; then
    files="$(cd "${EDGE_REPO_ROOT}" && find ${paths} -type f 2>/dev/null | sort)"
  fi
  [[ -z "${files}" ]] && { echo "MISSING-ARTIFACT"; return 0; }
  # Hash the concatenated, path-sorted file contents.
  ( cd "${EDGE_REPO_ROOT}" && printf '%s\n' ${files} | LC_ALL=C sort \
      | while IFS= read -r f; do printf '%s\0' "$f"; cat "$f"; done ) \
    | sha256
}

# edge_check_interface <name>
# Boot gate: verify the committed artifact is the reconciled one (hash-pin).
# Fast — no build, no DB. The authoritative artifact-matches-producer check is
# the CI layer (see _edge_artifact_path). Returns non-zero (caller fails closed)
# when the artifact differs from its reconciled baseline.
edge_check_interface() {
  local name="$1"
  local stored="${EDGE_HASH_DIR}/${name}.hash"
  local current
  current="$(_edge_artifact_hash "${name}")" || {
    echo "  ✗ edge[iface:${name}]: unknown interface (no artifact mapping)" >&2
    return 1
  }

  if [[ "${current}" == "MISSING-ARTIFACT" ]]; then
    echo "  ✗ edge[iface:${name}]: committed artifact not found ($(_edge_artifact_path "${name}" | sed "s#${EDGE_REPO_ROOT}/##")) — regenerate it" >&2
    return 1
  fi

  if [[ ! -f "${stored}" ]]; then
    edge_reconcile_interface "${name}"
    return $?
  fi

  if [[ "$(cat "${stored}")" != "${current}" ]]; then
    cat >&2 <<EOF
  ✗ edge[iface:${name}]: STALE — committed artifact changed since last reconcile.
    Artifact: $(_edge_artifact_path "${name}" | sed "s#${EDGE_REPO_ROOT}/##")
    Fix:      $(_edge_regen_hint "${name}")
    Then:     scripts/session-up.sh reconcile
EOF
    return 1
  fi

  echo "  ✓ edge[iface:${name}]: current"
  return 0
}

# edge_reconcile_interface <name> — re-stamp the baseline after an intentional
# regenerate. Called by `session-up.sh reconcile`.
edge_reconcile_interface() {
  local name="$1" current
  current="$(_edge_artifact_hash "${name}")" || return 1
  [[ "${current}" == "MISSING-ARTIFACT" ]] && { echo "  ✗ ${name}: committed artifact missing — regenerate it first" >&2; return 1; }
  mkdir -p "${EDGE_HASH_DIR}"
  printf '%s\n' "${current}" > "${EDGE_HASH_DIR}/${name}.hash"
  echo "  ✓ reconciled iface:${name}"
}

# ── Data: template clone with freshness budget ───────────────────────────────

# Parse host:port:user:db out of a postgres URL (pure bash, mirrors start-proxy).
# Sets EDGE_DB_HOST/PORT/USER/NAME. Returns non-zero on an unparseable URL.
_edge_parse_pg_url() {
  local url="$1"
  [[ "${url}" == *@* ]] || return 1
  local creds="${url#*://}"; creds="${creds%%@*}"
  EDGE_DB_USER="${creds%%:*}"
  local hp="${url#*@}"; hp="${hp%%/*}"
  EDGE_DB_HOST="${hp%%:*}"
  if [[ "${hp}" == *:* ]]; then EDGE_DB_PORT="${hp##*:}"; else EDGE_DB_PORT="5432"; fi
  local rest="${url#*@}"; EDGE_DB_NAME="${rest#*/}"; EDGE_DB_NAME="${EDGE_DB_NAME%%\?*}"
  return 0
}

# edge_provision_data <template_db> <max_age_hours> <admin_url> [ephemeral_name]
# 1. Probe the admin DB; fail fast if unreachable (start-proxy convention).
# 2. Read provenance (meta.golden_provenance) from the template; refuse if older
#    than max_age_hours — stale data is not true data.
# 3. CREATE DATABASE <ephemeral> TEMPLATE <template_db> (instant, real, identical
#    every session). Echo the ephemeral DB's URL on stdout for the caller to use.
edge_provision_data() {
  local template="$1" max_age_h="$2" admin_url="$3" ephemeral="${4:-}"

  if ! command -v psql >/dev/null 2>&1; then
    echo "  ✗ edge[data]: psql not on PATH (needed to clone the golden template)" >&2
    return 1
  fi
  _edge_parse_pg_url "${admin_url}" || {
    echo "  ✗ edge[data]: cannot parse MIKE_TEST_ADMIN_URL" >&2; return 1
  }
  if ! (exec 3<>"/dev/tcp/${EDGE_DB_HOST}/${EDGE_DB_PORT}") 2>/dev/null; then
    echo "  ✗ edge[data]: admin DB at ${EDGE_DB_HOST}:${EDGE_DB_PORT} unreachable" >&2
    echo "    Bring a Postgres up (scripts/local-db/up.sh) and set MIKE_TEST_ADMIN_URL=.../postgres" >&2
    return 1
  fi

  # Provenance lives in the template itself: meta.golden_provenance(captured_at).
  local template_url="${admin_url%/*}/${template}"
  local captured_at age_h
  captured_at="$(psql "${template_url}" -tAc \
    "SELECT max(captured_at) FROM meta.golden_provenance" 2>/dev/null | tr -d '[:space:]')"
  if [[ -z "${captured_at}" || "${captured_at}" == "null" ]]; then
    cat >&2 <<EOF
  ✗ edge[data]: golden template '${template}' has no provenance.
    Build it from a real capture: scripts/session-data-refresh.sh --template ${template} --from <dump|--source-url URL>
EOF
    return 1
  fi
  age_h="$(psql "${template_url}" -tAc \
    "SELECT round(extract(epoch FROM (now() - max(captured_at))) / 3600.0, 1) FROM meta.golden_provenance" \
    2>/dev/null | tr -d '[:space:]')"
  # Numeric staleness check (awk handles the float compare portably).
  if awk "BEGIN{exit !(${age_h:-99999} > ${max_age_h})}"; then
    cat >&2 <<EOF
  ✗ edge[data]: golden template '${template}' is ${age_h}h old (budget ${max_age_h}h) — stale.
    Refresh: scripts/session-data-refresh.sh --template ${template} --from <dump|--source-url URL>
EOF
    return 1
  fi

  # Mint the per-session clone. Short, collision-resistant suffix from the PID +
  # epoch (no Math.random equivalent needed; uniqueness, not secrecy).
  [[ -z "${ephemeral}" ]] && ephemeral="${template}_session_$$_$(date +%s)"
  if ! psql "${admin_url}" -v ON_ERROR_STOP=1 -qc \
      "CREATE DATABASE \"${ephemeral}\" TEMPLATE \"${template}\"" >/dev/null 2>&1; then
    echo "  ✗ edge[data]: clone failed (is anything connected to '${template}'?)" >&2
    return 1
  fi

  echo "  ✓ edge[data]: cloned '${template}' (${age_h}h old) → '${ephemeral}'" >&2
  # The ONLY thing on stdout: the ephemeral DB URL, for the caller to capture.
  echo "${admin_url%/*}/${ephemeral}"
}
