#!/usr/bin/env bash
# deploy-proxy.sh — validated, fail-safe cutover of the mike Rust binary.
#
# WHY THIS EXISTS
# ---------------
# start-proxy.sh kills the running proxy (free_port) BEFORE the new binary has
# run its on-startup migrations and bound its ports. If a migration fails, or
# the new binary can't boot, you're left with nothing running — and possibly a
# half-touched DB. Recovering by hand is the "massive undertaking" this script
# removes.
#
# This script never touches the live proxy until a freshly-built binary has
# PROVEN it can boot:
#
#   1. snapshot the currently-deployed binary  -> .deploy/mike.prev  (rollback copy)
#   2. build the new binary                      (old proxy keeps serving; build failure aborts here)
#   3. SMOKE TEST: start the new binary on ALT ports against the real DB.
#      This exercises migrations + the sequence allocator + config + bind, then
#      polls its /api/health. Failure => kill the alt instance, the live proxy
#      was NEVER touched, abort with the smoke log.
#   4. CUTOVER: stop the old proxy, start the new one on the real ports, poll
#      /api/health. If the new proxy fails to come healthy, restart the snapshot
#      binary (.prev) and report.
#
# PRECONDITION — migrations must be EXPAND-ONLY / backward-compatible.
# ------------------------------------------------------------------
# The smoke test applies migrations to the REAL (shared) database, so the still-
# running old proxy briefly serves against the new schema, and the rollback path
# runs the OLD binary against the already-migrated schema. Both are safe ONLY for
# additive migrations (new tables/columns/indexes, nullable adds, new views).
# A *successful but destructive* migration (DROP/RENAME COLUMN, retype, add
# NOT NULL without default) cannot be undone by reverting the binary — this
# script's rollback CANNOT save you there. `CREATE INDEX CONCURRENTLY` is also
# non-transactional: a mid-build failure leaves an INVALID index needing a manual
# DROP. For those, snapshot the DB first (pg_dump) or accept planned downtime;
# do NOT rely on this script's rollback. The migration runner itself waits up to
# MIKE_MIGRATION_LOCK_TIMEOUT_SECS (default 180s) for a lock on a hot table; the
# live proxy's own connections keep their 5s lock_timeout throughout.
#
# This script manages the Rust proxy binary (the thing with the DB + the /v1
# surface Claude Code points at) AND the proxy-forward MITM sidecar (:9987) — the
# failover layer that covers the cutover window. The sidecar is (re)started only
# when it is stale or not running; an unchanged running one is left alive so the
# swap stays gap-free. The Vite SPA and Storybook are independent processes that
# talk to the inspect bind on a stable port; they keep running and auto-reconnect
# across the ~seconds-long swap. Use start-proxy.sh for first-run / dev (it
# launches the whole stack in the foreground); use THIS for redeploys.
#
# NOTE: do not run this while the proxy is supervised by a FOREGROUND
# start-proxy.sh — that supervisor's `wait $MIKE_PID` + EXIT trap will fire when
# this script stops the old proxy and tear down the SPA/Storybook (and try to
# free the new proxy's ports). Either run the proxy detached (this script does),
# or stop the foreground supervisor first.
#
# SMART-SKIP — an unchanged binary against a healthy live proxy is a no-op.
# ------------------------------------------------------------------------
# The proxy's behaviour and its embedded migrations are a pure function of the
# binary, so a byte-identical rebuild has nothing to cut over to. After building
# + staging, this script hashes the staged binary against the last-deployed one
# (.deploy/mike.deployed) and, if they match AND the live proxy answers
# /api/health, it leaves the running proxy untouched and exits 0 — no smoke test,
# no swap, no interruption. Pass --force to redeploy regardless.
#
# USAGE
#   ./scripts/deploy-proxy.sh                  # optimized (release) build, cutover only if changed
#   ./scripts/deploy-proxy.sh --debug          # unoptimized build: faster to compile, many times
#                                              # the CPU per request once running
#   ./scripts/deploy-proxy.sh --force          # always cut over, even if unchanged
#   ./scripts/deploy-proxy.sh --dry-run        # build + migrate-check + alt-port smoke test
#                                              # ONLY; never touches the live proxy, and
#                                              # skips the smoke boot when migrations are
#                                              # pending, so it never migrates the DB
#   ./scripts/deploy-proxy.sh --health-timeout=90   # override readiness wait (default 300s)
#
# Stop the deployed proxy later with ./scripts/stop-proxy.sh (frees the ports).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# Load .env so MIKE_DATABASE_URL (and other dev-time vars) reach migrate-check,
# the smoke instance, and the deployed proxy. No-op when .env is absent.
source "${SCRIPT_DIR}/lib/source-env.sh"
mike_source_env_preserving_exported "${PROJECT_ROOT}/.env"

# mike IS the egress to api.anthropic.com — it must connect directly and
# never route its own upstream through an ambient HTTP(S) proxy. A deploy run
# from inside a `cc` session inherits that alias's HTTPS_PROXY=http://127.0.0.1:9987,
# which points at the proxy-forward MITM sidecar; the proxy would then loop back
# through the sidecar and reject its interception cert (UnknownIssuer) on every
# request. Strip inherited proxy vars from every proxy launch below (smoke,
# cutover, rollback). The binary also sets reqwest .no_proxy(); this is defense
# in depth and keeps the smoke test honest.
unset HTTPS_PROXY https_proxy HTTP_PROXY http_proxy ALL_PROXY all_proxy NO_PROXY no_proxy

# Builds use the committed .sqlx offline cache, so no DB connection is needed at
# build time and there is no DATABASE_URL to set. The only DB var is
# MIKE_DATABASE_URL (read at runtime by the binary + migrate-check). To refresh
# the cache after changing a query: scripts/sqlx-prepare (never cargo sqlx
# prepare, which empties .sqlx while it runs).

# Rustup binaries live in ~/.cargo/bin; non-login shells (Claude, CI) may not
# have it on PATH. Mirror start-proxy.sh's silent prepend.
if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

# ── arg parsing ──────────────────────────────────────────────────────────────
# Release by default: a debug proxy spends most of its CPU on debug-only checks
# while hashing and serialising multi-megabyte request bodies.
RELEASE_FLAG="--release"
BUILD_PROFILE="release"
DRY_RUN="false"
FORCE="false"
WAIT_MAX_SECONDS=300
for arg in "$@"; do
    case "${arg}" in
        --release)              RELEASE_FLAG="--release"; BUILD_PROFILE="release" ;;
        --debug)                RELEASE_FLAG=""; BUILD_PROFILE="debug" ;;
        --dry-run)              DRY_RUN="true" ;;
        --force)                FORCE="true" ;;
        --health-timeout=*)     WAIT_MAX_SECONDS="${arg#*=}" ;;
        *) echo "deploy-proxy.sh: unknown argument '${arg}' (expected --release, --debug, --dry-run, --force, or --health-timeout=N)" >&2; exit 2 ;;
    esac
done

DEPLOY_DIR="${PROJECT_ROOT}/.deploy"
mkdir -p "${DEPLOY_DIR}"

# ── one deploy at a time ─────────────────────────────────────────────────────
# Two deploys share .deploy/mike.staged, the smoke ports and the cutover. On
# 2026-10-01 two overlapped: one printed "No cutover needed — leaving the live
# proxy untouched" three seconds before the other restarted the proxy, and the
# binary that one cut over to was the first one's staged copy. A second deploy
# now waits for the first to finish, then does its own work against whatever
# that one left.
#
# The lock is an flock on file descriptor 9, so it is released when this script
# exits, however it exits. A process that outlives the script must not inherit
# it or the lock would outlive the deploy: every spawn below closes it (9>&-),
# and the units are started by systemd, which never sees it. Where there is no
# flock command (macOS) there is no lock, as before.
DEPLOY_LOCK="${DEPLOY_DIR}/deploy.lock"
DEPLOY_LOCK_WAIT_SECONDS="${DEPLOY_LOCK_WAIT_SECONDS:-1800}"
if command -v flock >/dev/null 2>&1; then
    exec 9>>"${DEPLOY_LOCK}"
    if ! flock -n 9; then
        echo "==> Another deploy is running ($(tail -n1 "${DEPLOY_LOCK}" 2>/dev/null || true)) — waiting up to ${DEPLOY_LOCK_WAIT_SECONDS}s for it to finish"
        if ! flock -w "${DEPLOY_LOCK_WAIT_SECONDS}" 9; then
            echo "==> The other deploy did not finish within ${DEPLOY_LOCK_WAIT_SECONDS}s — giving up. Nothing was touched." >&2
            exit 1
        fi
        echo "==> The other deploy finished — carrying on with this one."
    fi
    printf 'pid %s, started %s\n' "$$" "$(date '+%H:%M:%S')" >"${DEPLOY_LOCK}"
fi

PREV_BIN="${DEPLOY_DIR}/mike.prev"
# The freshly-built binary is STAGED into DEPLOY_DIR immediately after the
# build and every later step (config eval, migrate-check, smoke, cutover) runs
# the staged copy — never the CARGO_TARGET_DIR artifact. The target dir is
# shared across concurrent sessions, so its mike file can be silently
# overwritten by another build between our smoke test and cutover (this
# happened: a stale worktree binary missing an applied migration got cut over,
# and its .prev snapshot was equally stale, so rollback failed and the proxy
# went down). DEPLOYED_BIN records the binary that last passed cutover health
# and is the only trustworthy .prev source.
STAGED_BIN="${DEPLOY_DIR}/mike.staged"
DEPLOYED_BIN="${DEPLOY_DIR}/mike.deployed"
PID_FILE="${DEPLOY_DIR}/mike.pid"
MIKE_LOG="${DEPLOY_DIR}/mike.log"
SMOKE_LOG="${DEPLOY_DIR}/smoke.log"

# ── helpers ──────────────────────────────────────────────────────────────────

# Shared with start-proxy.sh — free_port, bin_sha, bins_differ, ensure_proxy_forward.
# Single source of truth so the two launchers cannot drift. Reads globals
# (PROJECT_ROOT, TARGET_DIR, BUILD_PROFILE, RELEASE_FLAG, FORCE) at call time,
# all of which are set above / before the call sites.
# shellcheck source=lib/proxy-runtime.sh
source "${SCRIPT_DIR}/lib/proxy-runtime.sh"
source "${SCRIPT_DIR}/lib/stack-units.sh"
USE_UNITS=false
if stack_units_enabled; then
    USE_UNITS=true
fi

# Poll an inspect bind's /api/health until it answers 200 or we time out.
# $1 = inspect addr (host:port), $2 = pid to watch (dies early => fail fast),
# $3 = human label. Returns 0 healthy, 1 otherwise.
wait_healthy() {
    local addr="$1" watch_pid="$2" label="$3" waited=0
    while :; do
        if ! kill -0 "${watch_pid}" 2>/dev/null; then
            echo "==> ${label} exited before becoming healthy" >&2
            return 1
        fi
        if curl --silent --output /dev/null --fail --max-time 1 \
            "http://${addr}/api/health"; then
            return 0
        fi
        sleep 0.2
        waited=$(awk "BEGIN {print ${waited} + 0.2}")
        if awk "BEGIN {exit !(${waited} >= ${WAIT_MAX_SECONDS})}"; then
            echo "==> ${label} did not become healthy within ${WAIT_MAX_SECONDS}s" >&2
            return 1
        fi
    done
}

# New-session detach, portable: nohup execs its argv[0] directly, so this must
# resolve to a real command, never a shell function. Real setsid where the OS
# ships it (Linux); a python3 os.setsid()+execvp shim where it does not (macOS
# has no setsid binary). Either way the shim execs in place — same process,
# no extra fork — so $! at the nohup call site stays the target binary's own
# pid, matching plain setsid's non -f behaviour.
if command -v setsid >/dev/null 2>&1; then
    DETACH_CMD=(setsid)
elif command -v python3 >/dev/null 2>&1; then
    DETACH_CMD=(python3 -c '
import os, sys
os.setsid()
os.execvp(sys.argv[1], sys.argv[1:])
')
else
    echo "deploy-proxy.sh: neither setsid nor python3 is on PATH — no portable way to" >&2
    echo "  detach the proxy into its own session on this machine. Install one of them" >&2
    echo "  (util-linux for setsid, or python3) before deploying here." >&2
    exit 2
fi

# Keep the `mike` command on PATH on the build that is live. `make install`
# installs only mike-mcp, so the CLI in ~/.local/bin stayed on whatever was last
# copied there by hand; `mike spawn` then refuses to run, because a CLI whose
# migration ledger differs from the database's will not connect (#359). Only an
# existing install is refreshed — nothing is put on PATH that was not there —
# and only from the main checkout, never from a linked worktree's build. Never
# fails a deploy: the proxy is already live when this runs.
refresh_mike_cli() {
    local cli="${INSTALL_DIR:-${HOME}/.local/bin}/mike"
    [[ -f "${cli}" && -x "${DEPLOYED_BIN}" ]] || return 0
    [[ "$(git -C "${PROJECT_ROOT}" rev-parse --git-dir 2>/dev/null)" \
        == "$(git -C "${PROJECT_ROOT}" rev-parse --git-common-dir 2>/dev/null)" ]] || return 0
    bins_differ "${DEPLOYED_BIN}" "${cli}" || return 0
    if rm -f "${cli}" && cp "${DEPLOYED_BIN}" "${cli}"; then
        echo "==> The mike command in ${cli%/*} was behind the deployed build — updated it"
    else
        echo "==> Could not update ${cli} to the deployed build — copy ${DEPLOYED_BIN} there by hand" >&2
    fi
}

# Single-shot liveness probe of an inspect bind (host:port). 0 = a proxy is
# answering /api/health now. Distinct from wait_healthy: no retry/watch loop —
# just "is something live right now" for the smart-skip decision.
proxy_is_healthy() {
    curl --silent --output /dev/null --fail --max-time 2 "http://$1/api/health"
}

# Confirm the freshly cut-over process reports the commit this deploy built
# from (mike#131). Healthy-and-answering is not the same as running-your-code:
# a stale binary served from a cache answers /api/health just as happily.
# $1 = inspect addr. Non-fatal by design — the cutover already succeeded, so a
# mismatch is a loud warning, not a rollback trigger.
verify_live_build_sha() {
    local addr="$1" body expected live dirty suffix=""
    body="$(curl --silent --fail --max-time 2 "http://${addr}/api/health" || true)"
    expected="$(git -C "${PROJECT_ROOT}" rev-parse HEAD 2>/dev/null || true)"
    # Whitespace-tolerant: a pretty-printed body must not degrade a real
    # mismatch into the softer "could not read a sha" warning.
    live="$(printf '%s' "${body}" \
        | grep -oE '"sha"[[:space:]]*:[[:space:]]*"[0-9a-f]{40}"' \
        | head -1 | grep -oE '[0-9a-f]{40}')"

    if [[ -z "${live}" ]]; then
        echo "==> WARNING: live proxy did not report a build sha on /api/health." >&2
        echo "    Built before mike#131 landed, built without git metadata, or the health shape changed." >&2
        return 0
    fi
    if printf '%s' "${body}" | grep -qE '"dirty"[[:space:]]*:[[:space:]]*true'; then
        dirty="true"
        suffix=" — built from a DIRTY tree, so uncommitted work is live"
    fi
    if [[ -z "${expected}" ]]; then
        echo "==> Live build: ${live} (no local git HEAD to compare against)${suffix}"
        return 0
    fi
    if [[ "${live}" != "${expected}" ]]; then
        echo "==> WARNING: the live proxy is NOT running this checkout's commit." >&2
        echo "    expected: ${expected}" >&2
        echo "    live:     ${live}${dirty:+ (dirty build)}" >&2
        echo "    A stale binary answered the health check. Investigate before trusting this deploy." >&2
        return 0
    fi
    echo "==> Live build: ${live} matches HEAD${suffix}"
}

# Smoke-test instance pid — cleaned up by the trap on any early exit. The trap
# deliberately NEVER frees the real proxy ports: an abort must leave the live
# proxy exactly as it was.
SMOKE_PID=""
CUTOVER_DONE="false"
# shellcheck disable=SC2329  # invoked indirectly via `trap cleanup EXIT INT TERM`
cleanup() {
    local code=$?
    if [[ -n "${SMOKE_PID}" ]] && kill -0 "${SMOKE_PID}" 2>/dev/null; then
        echo "==> Cleaning up smoke-test instance (pid ${SMOKE_PID})"
        kill "${SMOKE_PID}" 2>/dev/null || true
        # Block until the child is reaped so its two binds are released before
        # we return — a bare `kill` exits faster than the socket closes.
        wait "${SMOKE_PID}" 2>/dev/null || true
    fi
    # Belt-and-suspenders: release the alt ports even if the smoke process
    # detached or was already reaped. Guarded — an early abort (e.g. build
    # failure) happens before these vars are assigned.
    [[ -n "${ALT_PROXY_PORT:-}" ]] && free_port "${ALT_PROXY_PORT}"
    [[ -n "${ALT_INSPECT_PORT:-}" ]] && free_port "${ALT_INSPECT_PORT}"
    if [[ "${code}" -ne 0 && "${CUTOVER_DONE}" == "false" ]]; then
        echo "==> Aborted before cutover — the live proxy was not touched." >&2
    fi
    exit "${code}"
}
trap cleanup EXIT INT TERM

# ── 1. snapshot the currently-deployed binary (rollback copy) ────────────────
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -n1)"
[[ -z "${TARGET_DIR}" ]] && TARGET_DIR="${PROJECT_ROOT}/target"
MIKE_BIN="${TARGET_DIR}/${BUILD_PROFILE}/mike"

# Prefer the recorded last-deployed binary; the target-dir artifact is only a
# bootstrap fallback (first deploy on a machine). rm before cp: on macOS,
# cp-ing over a file a live process is executing SIGKILLs that process — a
# rolled-back proxy may be running the .prev inode right now.
if [[ -x "${DEPLOYED_BIN}" ]]; then
    rm -f "${PREV_BIN}"
    cp "${DEPLOYED_BIN}" "${PREV_BIN}"
    echo "==> Snapshotted last-deployed binary -> ${PREV_BIN} (rollback copy)"
elif [[ -x "${MIKE_BIN}" ]]; then
    rm -f "${PREV_BIN}"
    cp "${MIKE_BIN}" "${PREV_BIN}"
    echo "==> No deployed-binary record; snapshotted target-dir binary -> ${PREV_BIN} (rollback copy, best effort)"
else
    echo "==> No existing ${BUILD_PROFILE} binary to snapshot — rollback will be unavailable"
    rm -f "${PREV_BIN}"
fi

# ── 2. build the new binary (old proxy keeps serving) ────────────────────────
# Under `set -e` a build failure aborts the script HERE, before anything touches
# the live proxy. The cleanup trap kills no real process.
# Ensure the build accelerators wired into the workspace toolchain config (sccache, lld)
# are present first — without them cargo fails cryptically.
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/preflight-toolchain.sh"
ensure_build_toolchain
echo "==> Building mike ${RELEASE_FLAG:+(release)}"
# Build against the committed .sqlx offline cache — this script runs with no
# DATABASE_URL (only MIKE_DATABASE_URL, read at runtime), so without offline
# mode the sqlx query macros try to validate online, find no DATABASE_URL, and
# abort any build that actually recompiles mike. Default to offline; an operator
# refreshing the cache after a query change can override with SQLX_OFFLINE=false.
# Refresh the cache with: cargo sqlx prepare --workspace --database-url "$MIKE_DATABASE_URL" -- --all-targets.
SQLX_OFFLINE="${SQLX_OFFLINE:-true}" cargo build ${RELEASE_FLAG} --manifest-path crates/mike/Cargo.toml --quiet
echo "==> Build OK"

# Stage the artifact into DEPLOY_DIR at once — everything after this line runs
# the staged copy, immune to concurrent builds clobbering the target dir.
rm -f "${STAGED_BIN}"
cp "${MIKE_BIN}" "${STAGED_BIN}"
chmod +x "${STAGED_BIN}"
echo "==> Staged binary -> ${STAGED_BIN}"

# ── resolve effective config from the NEW binary (authoritative port source) ─
eval "$("${STAGED_BIN}" print-config --shell)"

# In normal dev the SPA port belongs to an independent `vite dev` process
# (start-proxy.sh exports MIKE_SPA_ENABLED=0 for exactly this reason), and
# every instance this script launches — smoke, cutover, rollback — would then
# die on "Address already in use". Yield the port when something already serves
# it, rather than disabling the SPA host outright: with no vite running, the
# binary is the only thing that would serve the UI. Evaluated after the eval
# above, which restores the config's own spa_enabled.
SPA_PORT="${MIKE_SPA_BIND##*:}"
if [[ "${MIKE_SPA_ENABLED}" != "0" && -n "${SPA_PORT}" ]] \
    && lsof -nP -iTCP:"${SPA_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "==> SPA port ${SPA_PORT} is already served (vite dev) — launching with the SPA host off"
    export MIKE_SPA_ENABLED=0
fi

MIKE_ADDR="${MIKE_LISTEN_ADDR}"
INSPECT_ADDR="${MIKE_INSPECT_ADDR}"
MIKE_PORT="${MIKE_ADDR##*:}"
INSPECT_PORT="${INSPECT_ADDR##*:}"

# ── 2.6 SMART-SKIP: identical binary already live and healthy ─────────────────
# Behaviour + embedded migrations are a pure function of the binary, so a
# byte-identical rebuild against a healthy live proxy has nothing to change.
# Skipping avoids a needless cutover — a real ~seconds interruption plus a full
# smoke boot — on the common "I only touched the frontend / another crate" run.
# --force, an absent deployed-binary record, a hash mismatch, or an unhealthy /
# missing live proxy all fall through to the full safe cutover below.
# DEPLOYED_BIN is written only after a cutover's own health check passes, so it
# tracks exactly what is live; pairing the hash match with a live /api/health
# probe guards against a stale record or a crashed proxy.
if [[ "${DRY_RUN}" == "false" && "${FORCE}" == "false" && -x "${DEPLOYED_BIN}" ]] \
    && [[ -n "$(bin_sha "${STAGED_BIN}")" ]] \
    && [[ "$(bin_sha "${STAGED_BIN}")" == "$(bin_sha "${DEPLOYED_BIN}")" ]] \
    && proxy_is_healthy "${INSPECT_ADDR}" \
    && { ! ${USE_UNITS} || unit_active mike-stack-proxy; }; then
    # Under the units the pid file is only as fresh as the last cutover: after a
    # reboot, or a restart by systemd, it names a process that is gone. Ask the
    # unit, which is the one that knows.
    if ${USE_UNITS}; then
        LIVE_PID="$(unit_main_pid mike-stack-proxy)"
    else
        LIVE_PID="$(cat "${PID_FILE}" 2>/dev/null || true)"
    fi
    echo "==> mike is byte-identical to the running proxy${LIVE_PID:+ (pid ${LIVE_PID})} and /api/health is green."
    echo "==> No cutover needed — leaving the live proxy untouched (use --force to redeploy anyway)."
    CUTOVER_DONE="true"   # nothing to roll back; suppress the trap's abort notice
    # Even when mike is unchanged, make sure the sidecar is up and current.
    ensure_proxy_forward
    if ${USE_UNITS}; then
        ensure_stack_units_running
    fi
    refresh_mike_cli || true
    exit 0
fi

# Alt ports for the smoke test — real port + 10000 keeps them memorable and well
# clear of the real range. Loopback only; the smoke instance receives no traffic.
ALT_PROXY_PORT=$(( MIKE_PORT + 10000 ))
ALT_INSPECT_PORT=$(( INSPECT_PORT + 10000 ))
ALT_PROXY_ADDR="127.0.0.1:${ALT_PROXY_PORT}"
ALT_INSPECT_ADDR="127.0.0.1:${ALT_INSPECT_PORT}"

# ── 2.5 PRE-MIGRATION ASSESSMENT (read-only, against the TARGET DB) ───────────
# Before we boot anything, ask the new binary whether its embedded migrations
# are even compatible with what's already in the target's `_sqlx_migrations`.
# Catches the common failure where an applied migration's .sql was edited
# (checksum mismatch → sqlx refuses to migrate → the smoke boot would crash)
# and surfaces it as a precise diagnostic instead of a generic boot failure.
# Read-only: no migration is applied. Failure aborts before anything is killed.
echo "==> Pre-migration assessment (read-only) against the target DB"
MIGRATE_CHECK_OUT="${DEPLOY_DIR}/migrate-check.out"
if ! "${STAGED_BIN}" migrate-check | tee "${MIGRATE_CHECK_OUT}"; then
    echo "==> Migration assessment FAILED — aborting before cutover. Target DB untouched." >&2
    exit 1   # before the smoke instance starts; trap kills nothing real
fi

# The smoke boot applies pending migrations to the live DB, so a dry run with any
# pending stops here (#671): the live ledger moving ahead of the deployed binary
# makes that binary unrestartable until the next real deploy.
if [[ "${DRY_RUN}" == "true" ]] \
    && (( $(pending_migration_count < "${MIGRATE_CHECK_OUT}") > 0 )); then
    echo "==> --dry-run: migrations are pending (listed above), so the smoke boot is skipped —"
    echo "    it would apply them to the live database. A real deploy applies them and boots."
    exit 0
fi

# ── 3. SMOKE TEST: boot the new binary on alt ports against the real DB ───────
echo "==> Pre-flight: freeing alt ports ${ALT_PROXY_PORT}, ${ALT_INSPECT_PORT}"
free_port "${ALT_PROXY_PORT}"
free_port "${ALT_INSPECT_PORT}"

echo "==> Smoke test: starting new binary on alt ports (migrations + allocator + bind)"
echo "    alt proxy bind:   http://${ALT_PROXY_ADDR}/v1/"
echo "    alt inspect bind: http://${ALT_INSPECT_ADDR}/api/"
# Inherit the resolved MIKE_* config, override only the two bind addrs so the
# smoke instance can coexist with the live proxy. nextval()-based id allocation
# is concurrency-safe, so two instances on one DB just take disjoint id batches.
# env -u: a concurrent SessionEnd reaping this on an inherited session tag would
# surface as a spurious SMOKE TEST FAILED and abort a healthy deploy.
# MIKE_SMOKE_RUN=1 (issue #126): this instance boots against the LIVE
# capture DB just to prove the binary boots — it must never force-close
# live in-flight rows, re-enqueue live mailbox mail, or spawn a background
# worker that touches shared state or messages a live session. Set ONLY
# on this launch; the cutover and rollback launches below run for real
# and must keep every startup reconciliation + worker on.
MIKE_LISTEN_ADDR="${ALT_PROXY_ADDR}" MIKE_INSPECT_ADDR="${ALT_INSPECT_ADDR}" MIKE_SMOKE_RUN=1 \
    env -u CLAUDE_CODE_SESSION_ID "${STAGED_BIN}" >"${SMOKE_LOG}" 2>&1 9>&- &
SMOKE_PID=$!

if ! wait_healthy "${ALT_INSPECT_ADDR}" "${SMOKE_PID}" "smoke-test instance"; then
    echo "==> SMOKE TEST FAILED — see ${SMOKE_LOG}:" >&2
    tail -n 30 "${SMOKE_LOG}" >&2 || true
    echo "==> The live proxy was NOT touched. Nothing to roll back." >&2
    exit 1   # trap kills SMOKE_PID; real proxy untouched
fi
echo "==> Smoke test PASSED — new binary boots, migrations applied, /api/health green"

# ── --dry-run stops here: validated, live proxy never touched ────────────────
if [[ "${DRY_RUN}" == "true" ]]; then
    echo "==> --dry-run: validation complete, no migration applied, no cutover performed."
    exit 0   # trap kills the smoke instance
fi

# ── 4. CUTOVER ───────────────────────────────────────────────────────────────
# Bring the sidecar up FIRST so it covers the imminent :9988 gap (a Remote-Control
# client keeps flowing DIRECT to Anthropic while the proxy swaps).
ensure_proxy_forward
echo "==> Cutover: stopping smoke instance, swapping live proxy"
kill "${SMOKE_PID}" 2>/dev/null || true
wait "${SMOKE_PID}" 2>/dev/null || true
SMOKE_PID=""
free_port "${ALT_PROXY_PORT}"
free_port "${ALT_INSPECT_PORT}"

# Stop the currently-live proxy. Under the units, stop the unit FIRST: with
# Restart=always, killing its process would just bring the old binary back.
if ${USE_UNITS}; then
    install_stack_units
    systemctl --user stop mike-stack-proxy.service 2>/dev/null || true
fi
# Then a PID we wrote on a prior nohup deploy, then whatever still holds the ports.
if [[ -f "${PID_FILE}" ]]; then
    OLD_PID="$(cat "${PID_FILE}" 2>/dev/null || true)"
    if [[ -n "${OLD_PID}" ]] && kill -0 "${OLD_PID}" 2>/dev/null; then
        echo "==> Stopping previous deployed proxy (pid ${OLD_PID})"
        kill "${OLD_PID}" 2>/dev/null || true
        # The proxy holds the installation lock until it exits, and its shutdown
        # drains connections for up to 10s under a 60s hard deadline
        # (crates/mike/src/serve/shutdown.rs). Starting the new binary before it
        # has gone fails with "already has a daemon owner", and so does the
        # rollback, so wait out the whole deadline. A process still alive after
        # that is hung and is killed so the lock is released.
        for _ in $(seq 1 140); do kill -0 "${OLD_PID}" 2>/dev/null || break; sleep 0.5; done
        if kill -0 "${OLD_PID}" 2>/dev/null; then
            echo "==> Previous proxy (pid ${OLD_PID}) still alive 70s after SIGTERM — killing it" >&2
            kill -9 "${OLD_PID}" 2>/dev/null || true
            for _ in $(seq 1 10); do kill -0 "${OLD_PID}" 2>/dev/null || break; sleep 0.2; done
        fi
    fi
fi
echo "==> Freeing real ports ${MIKE_PORT}, ${INSPECT_PORT}"
free_port "${MIKE_PORT}"
free_port "${INSPECT_PORT}"

# A proxy deployed from a linked worktree runs with that slot as its cwd and its
# .deploy/ as its exe. Removing the slot later leaves the shared daemon on a
# deleted cwd (2026-09-23: spawn_session failed with "reading the working
# directory"). Say so loudly; redeploy from the main checkout before removing it.
GIT_DIR_ABS="$(git -C "${PROJECT_ROOT}" rev-parse --absolute-git-dir 2>/dev/null || true)"
GIT_COMMON_ABS="$(cd "${PROJECT_ROOT}" && cd "$(git rev-parse --git-common-dir 2>/dev/null || echo .)" && pwd -P)"
if [[ -n "${GIT_DIR_ABS}" && "${GIT_DIR_ABS}" != "${GIT_COMMON_ABS}" ]]; then
    echo "==> WARNING: deploying the shared proxy from linked worktree ${PROJECT_ROOT}." >&2
    echo "    The live daemon will run from this slot. Do not 'git worktree remove' it" >&2
    echo "    until the proxy is redeployed from the main checkout." >&2
fi

# Start the new proxy DETACHED so it survives this script's exit. setsid +
# nohup so it isn't a child of this shell; logs to MIKE_LOG.
echo "==> Starting new proxy on real ports (detached; log -> ${MIKE_LOG})"
# Rotate rather than truncate: the restart that ENDS an outage would otherwise
# delete the only log of it (artifacts/2026-07-29/proxy-api-errors-brief.md).
[[ -s "${MIKE_LOG}" ]] && mv -f "${MIKE_LOG}" "${MIKE_LOG}.1"
# env -u: an inherited session tag lets the SessionEnd reaper kill this shared daemon.
# setsid: a NEW SESSION, not just a new process. Without it the proxy stays in the
# launcher's session/process group, and a kill of that group (a harness stopping
# the shell that ran this script, 2026-09-15) takes the live proxy down with it.
# setsid without -f execs in place, so $! is still the proxy's own pid.
if ${USE_UNITS}; then
    rm -f "${DEPLOYED_BIN}"
    cp "${STAGED_BIN}" "${DEPLOYED_BIN}"
    unit_restart mike-stack-proxy
    NEW_PID="$(unit_main_pid mike-stack-proxy)"
else
    nohup "${DETACH_CMD[@]}" env -u CLAUDE_CODE_SESSION_ID "${STAGED_BIN}" >"${MIKE_LOG}" 2>&1 9>&- &
    NEW_PID=$!
    disown "${NEW_PID}" 2>/dev/null || true
fi
echo "${NEW_PID}" >"${PID_FILE}"

if wait_healthy "${INSPECT_ADDR}" "${NEW_PID}" "new proxy"; then
    CUTOVER_DONE="true"
    verify_live_build_sha "${INSPECT_ADDR}"
    # Record what is now live so the NEXT deploy snapshots a trustworthy .prev.
    # cp (not mv) keeps the running process's exec path intact; rm-first is the
    # macOS running-binary guard.
    rm -f "${DEPLOYED_BIN}"
    cp "${STAGED_BIN}" "${DEPLOYED_BIN}"
    if ${USE_UNITS}; then
        ensure_stack_units_running
    fi
    refresh_mike_cli || true
    git -C "${PROJECT_ROOT}" rev-parse HEAD >"${DEPLOY_DIR}/mike.deployed.commit" 2>/dev/null || true
    echo
    echo "==> Cutover complete. New proxy is live and healthy (pid ${NEW_PID})."
    echo "    proxy bind:   http://${MIKE_ADDR}/v1/"
    echo "    inspect bind: http://${INSPECT_ADDR}/api/"
    echo "    sidecar:      http://127.0.0.1:9987/  (proxy-forward — Remote-Control failover; CA: ${MIKE_FORWARD_CA#"${PROJECT_ROOT}/"})"
    echo "    log:          ${MIKE_LOG}"
    echo "    stop with:    ./scripts/stop-proxy.sh"
    echo "    The SPA / Storybook (if running) reconnect automatically."
    exit 0
fi

# ── rollback: new proxy never came healthy ───────────────────────────────────
echo "==> New proxy failed health check — see ${MIKE_LOG}:" >&2
tail -n 30 "${MIKE_LOG}" >&2 || true
if ${USE_UNITS}; then
    systemctl --user stop mike-stack-proxy.service 2>/dev/null || true
fi
kill "${NEW_PID}" 2>/dev/null || true
free_port "${MIKE_PORT}"
free_port "${INSPECT_PORT}"

if [[ -x "${PREV_BIN}" ]]; then
    echo "==> Rolling back to previous binary (${PREV_BIN})" >&2
    echo "    NOTE: rollback is only safe if this deploy's migrations were additive." >&2
    if ${USE_UNITS}; then
        rm -f "${DEPLOYED_BIN}"
        cp "${PREV_BIN}" "${DEPLOYED_BIN}"
        unit_restart mike-stack-proxy
        ROLLBACK_PID="$(unit_main_pid mike-stack-proxy)"
    else
        nohup "${DETACH_CMD[@]}" env -u CLAUDE_CODE_SESSION_ID "${PREV_BIN}" >"${MIKE_LOG}" 2>&1 9>&- &
        ROLLBACK_PID=$!
        disown "${ROLLBACK_PID}" 2>/dev/null || true
    fi
    echo "${ROLLBACK_PID}" >"${PID_FILE}"
    if wait_healthy "${INSPECT_ADDR}" "${ROLLBACK_PID}" "rolled-back proxy"; then
        CUTOVER_DONE="true"   # a proxy is live again; don't print the abort notice
        rm -f "${DEPLOYED_BIN}"
        cp "${PREV_BIN}" "${DEPLOYED_BIN}"
        echo "==> Rollback succeeded — previous binary is live (pid ${ROLLBACK_PID})." >&2
        echo "    Investigate the failed build before retrying. Log: ${MIKE_LOG}" >&2
        exit 1
    fi
    echo "==> ROLLBACK ALSO FAILED. Proxy is DOWN. Inspect ${MIKE_LOG} and ${SMOKE_LOG}." >&2
    exit 1
fi

echo "==> No rollback binary available. Proxy is DOWN. Inspect ${MIKE_LOG}." >&2
exit 1
