#!/usr/bin/env bash
# Day-to-day dev launcher for the mike stack.
#
# Incremental by default: each Rust component is rebuilt (cargo is incremental,
# so cheap when nothing changed) but its disruptive follow-up runs ONLY when the
# freshly-built binary differs from what is already live:
#   1. mike-mcp     — reinstalled to ~/.local/bin only when changed (an install
#                      forces a Claude Code restart; skip it when byte-identical).
#   2. mike        — cut over only when changed (deploy-proxy.sh smart-skips a
#                      byte-identical rebuild against a healthy live proxy).
#   3. proxy-forward — the HTTPS_PROXY failover sidecar; a running, unchanged
#                      sidecar is LEFT ALIVE so it can cover the mike swap
#                      window (routes Direct to Anthropic while :9988 is down).
#   4. Vite dev SPA (HMR) — an already-running one is LEFT ALIVE (vite HMR needs
#      no redeploy); only a down one is (re)launched. Storybook and the
#      architecture diagrams are not started: systemctl --user start
#      mike-stack-storybook mike-stack-arch when you want them.
#
# Who keeps what running: the proxy, the sidecar, the prompt assistant and the
# SPA are systemd user units (mike-stack-*, scripts/lib/stack-units.sh) that
# outlive this script and restart on failure. So the script builds, deploys,
# prints the summary and returns the terminal. Without systemd
# (MIKE_STACK_SUPERVISOR=nohup, macOS) the SPA runs in this terminal's
# foreground instead, and Ctrl-C stops just it. Stop everything with ./scripts/stop-proxy.sh.
#
# Pass --force to rebuild-and-redeploy every component unconditionally.
#
# Ports (defaults):
#   - Rust proxy bind    :9988   /v1/*    (Claude API surface)
#   - Rust inspect bind  :9989   /api/*   (REST + WebSocket, CORS on)
#   - proxy-forward      :9987   HTTPS_PROXY MITM sidecar (failover to Anthropic)
#   - Vite dev SPA       :9990   /        (SvelteKit SPA, HMR enabled)
#   - Storybook          :6006   /        (component / route story sandbox)
#   - Architecture       :9986   /        (the LikeC4 model in docs/architecture, redrawn on save)
#
# Pre-flight: the capture DB is the Postgres that MIKE_DATABASE_URL (.env)
# addresses — on the development host the Docker container armitage-pg, started
# by scripts/capture-db/up.sh. The script probes it and, when it is unreachable,
# offers to retry, start anyway, or abort. Builds need no DB connection — they
# use the committed .sqlx offline cache.
#
# Running this script means "build and deploy": it never asks whether to. When
# the proxy is already up it says so, says which of the web UI, Storybook and
# the diagrams are down, and carries on — deploy-proxy.sh leaves a byte-identical
# proxy alone anyway, so a run that changes nothing interrupts nothing.
#
# One question is asked, and only when a person is there to answer: the capture
# database is unreachable. Instead of exiting immediately the script offers to
# retry the connection, which is usually all that is needed when the database
# container is still starting after a reboot. It cannot stall an unattended
# run: without a controlling terminal (an agent, a background job, CI) the
# script exits 1 on an unreachable database, as it always did. --yes answers it
# without asking (start anyway), as does START_PROXY_ASSUME_YES=1.
#
# Usage:
#   ./scripts/start-proxy.sh              # optimized (release) build, redeploy only what changed
#   ./scripts/start-proxy.sh --debug      # unoptimized build: faster to compile, many times
#                                         # the CPU per request once running
#   ./scripts/start-proxy.sh --force      # rebuild + redeploy every component
#   ./scripts/start-proxy.sh --yes        # never ask: start anyway when the database is unreachable
#   ./scripts/start-proxy.sh --check      # build + smoke-test only, no start
#                                         # (validates migrations/boot; proxy untouched)
#
# Zero-interruption redeploys: point Claude Code at the sidecar in Remote-Control
# mode (HTTPS_PROXY=http://127.0.0.1:9987, ANTHROPIC_BASE_URL unset). Then a
# mike cutover is invisible — the sidecar routes Direct to Anthropic for
# the ~seconds the proxy is swapping and re-attaches capture when it is healthy
# again. A client using ANTHROPIC_BASE_URL=:9988 talks straight to the proxy and
# still sees that brief window.
#
# Once running (the summary box at the end prints these, checked, with links):
#   - Web UI:      http://127.0.0.1:9990/      (runtime config at /settings)
#   - Storybook:   http://127.0.0.1:6006/
#   - Style guide: the Foundations/Style Guide stories in Storybook
#   - Diagrams:    http://127.0.0.1:9986/      (the architecture, from docs/architecture)
#   - REST API:    http://127.0.0.1:9989/api/requests
#   - Sessions:    `cc` (scripts/shell/cc.zsh), or ANTHROPIC_BASE_URL=http://127.0.0.1:9988 claude
#
# Logs (all under .deploy/):
#   - mike.log               the proxy's stdout+stderr (live tail target)
#   - smoke.log              last smoke-test run (alt-port boot validation)
#   - proxy-forward.log      the sidecar
#   - prompt-assistant.log   the prompt assistant
#   - spa.log, storybook.log, arch.log   the SPA, Storybook and the diagrams, when
#                            their units run them (in the foreground their output
#                            is in the terminal, not in a file)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

# Log locations (deploy-proxy.sh and the stack units write the files under here).
DEPLOY_DIR="${PROJECT_ROOT}/.deploy"
MIKE_LOG="${DEPLOY_DIR}/mike.log"

# Load .env (MIKE_DATABASE_URL, MIKE_STEERER_OPENROUTER_API_KEY, etc.).
source "${SCRIPT_DIR}/lib/source-env.sh"
mike_source_env_preserving_exported "${PROJECT_ROOT}/.env"

# This script's whole job is the dev workflow (vite dev with HMR on :9990,
# started below in "SPA + Storybook"), so disable mike's own SPA bind —
# vite owns :9990 in dev for HMR. Exported before deploy-proxy.sh runs so
# the detached proxy it starts inherits this. The committed ProxyConfig
# default stays enabled for the release/serving path (mike alone, no vite).
export MIKE_SPA_ENABLED=0

# Builds use the committed .sqlx offline cache, so no DB connection is needed at
# build time and there is no DATABASE_URL to set. The only DB var is
# MIKE_DATABASE_URL (read at runtime by the binary). To refresh the cache after
# changing a query: scripts/sqlx-prepare (never cargo sqlx prepare, which empties
# .sqlx while it runs).

# Rustup drops binaries in ~/.cargo/bin; non-login shells may not have it on PATH.
if ! command -v cargo >/dev/null 2>&1 && [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

# Release by default: a debug proxy spends most of its CPU on debug-only checks
# while hashing and serialising multi-megabyte request bodies.
RELEASE_FLAG="--release"
BUILD_PROFILE="release"
CHECK_ONLY="false"
FORCE="false"
# Answer every prompt below without asking. Set by --yes, or by exporting
# START_PROXY_ASSUME_YES=1 for a caller that wants the old, never-stops behaviour.
ASSUME_YES="${START_PROXY_ASSUME_YES:-false}"
[[ "${ASSUME_YES}" == "1" ]] && ASSUME_YES="true"
# How long a prompt waits for an answer before giving up and taking the
# nobody-is-there default, so a forgotten terminal cannot block a run forever.
PROMPT_TIMEOUT_SECONDS="${START_PROXY_PROMPT_TIMEOUT:-60}"
# Pause between database retries, long enough for a rebooting host to get its
# port listening but short enough to keep answering "r" a few times bearable.
DB_RETRY_PAUSE_SECONDS="${START_PROXY_DB_RETRY_PAUSE:-5}"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release)  RELEASE_FLAG="--release"; BUILD_PROFILE="release" ;;
        --debug)    RELEASE_FLAG=""; BUILD_PROFILE="debug" ;;
        --check)    CHECK_ONLY="true" ;;
        --force)    FORCE="true" ;;
        --yes|-y)   ASSUME_YES="true" ;;
        *) echo "start-proxy.sh: unknown argument '$1' (expected --release, --debug, --check, --force, or --yes)" >&2; exit 2 ;;
    esac
    shift
done

# Ask a single-key question on the user's terminal and echo the chosen key.
#
# This script is run by hand and also by agents, background jobs and CI, so a
# prompt must never hang a run that has nobody to answer it. A question is asked
# only when there is a real controlling terminal to ask on and --yes was not
# given. Otherwise the caller's fallback answer is taken at once and the run
# behaves exactly as it did before any prompting existed.
#
#   $1 question text   $2 accepted keys, for example "rsa"
#   $3 answer to take when nobody can be asked (no terminal, or timeout)
#   $4 answer to take when --yes was given
#
# The question goes to the terminal rather than to stdout, so a caller capturing
# this function's output receives only the chosen key.
# Read one KEY=value out of .env, printing nothing when the file or the key is
# absent. Worth a function because of `set -euo pipefail`: a bare sed over a
# missing .env fails, pipefail promotes that to the whole pipeline, and the
# script exits 1 having printed nothing at all — which is exactly what a fresh
# clone or a git worktree (neither has a .env yet) used to do.
env_value() {
    [[ -f "${PROJECT_ROOT}/.env" ]] || return 0
    sed -n "s/^$1=//p" "${PROJECT_ROOT}/.env" 2>/dev/null | tail -n1 | tr -d "\"'" || true
}

ask_choice() {
    local question="$1" keys="$2" auto_answer="$3" yes_answer="$4" reply=""

    if [[ "${ASSUME_YES}" == "true" ]]; then
        printf '%s\n' "${yes_answer}"
        return 0
    fi
    # Opening /dev/tty fails when the process has no controlling terminal, which
    # is what tells us an agent or a CI job is running this rather than a person.
    if ! exec 3<>/dev/tty 2>/dev/null; then
        printf '%s\n' "${auto_answer}"
        return 0
    fi

    while :; do
        printf '%s ' "${question}" >&3
        if ! IFS= read -r -t "${PROMPT_TIMEOUT_SECONDS}" -u 3 reply; then
            printf '\n==> No answer within %ss — taking "%s".\n' \
                "${PROMPT_TIMEOUT_SECONDS}" "${auto_answer}" >&3
            exec 3<&-
            printf '%s\n' "${auto_answer}"
            return 0
        fi
        reply="$(printf '%s' "${reply}" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
        # A bare Enter accepts the same answer an absent user would have got.
        [[ -z "${reply}" ]] && reply="${auto_answer}"
        if [[ ${#reply} -eq 1 && "${keys}" == *"${reply}"* ]]; then
            exec 3<&-
            printf '%s\n' "${reply}"
            return 0
        fi
        printf '==> Please answer with one of: %s\n' "$(printf '%s' "${keys}" | sed 's/./& /g')" >&3
    done
}

# True (0) when two binaries differ in content, or the target is missing — i.e.
# "an install/redeploy is warranted". Compares content hashes, not mtimes, so a
# no-op incremental rebuild that relinks nothing reads as identical and its
# disruptive follow-up (reinstall / restart) is skipped.
#
# free_port, bin_sha and bins_differ are shared with deploy-proxy.sh — single
# source of truth so the two launchers cannot drift. The sidecar helper
# (ensure_proxy_forward) lives there too and is run by deploy-proxy.sh in Step 2.
# shellcheck source=lib/proxy-runtime.sh
source "${SCRIPT_DIR}/lib/proxy-runtime.sh"
source "${SCRIPT_DIR}/lib/stack-units.sh"
source "${SCRIPT_DIR}/lib/managed-service.sh"

# ── Pre-flight: capture database reachability ────────────────────────────────
# Builds use the committed .sqlx offline cache (no DB needed at build time), but
# the proxy needs the capture DB live at boot. The DB URL lives only in
# MIKE_DATABASE_URL (.env). This script never starts the database itself: when
# it is unreachable it says how to start it and offers to retry.
DB_HOST=""
DB_PORT=""
# Config precedence is env > .env (matches the proxy binary). A session-up
# container/worktree exports MIKE_DATABASE_URL (a per-session DB clone); fall
# back to .env only when it is unset.
DB_URL="${MIKE_DATABASE_URL:-$(env_value MIKE_DATABASE_URL)}"
if [[ "${DB_URL}" == *@* ]]; then
    DB_HOSTPORT="${DB_URL#*@}"
    DB_HOSTPORT="${DB_HOSTPORT%%/*}"
    if [[ "${DB_HOSTPORT}" == *:* ]]; then
        DB_HOST="${DB_HOSTPORT%%:*}"
        DB_PORT="${DB_HOSTPORT##*:}"
    else
        DB_HOST="${DB_HOSTPORT}"
        DB_PORT="5432"
    fi
fi

if [[ -z "${DB_HOST}" || -z "${DB_PORT}" ]]; then
    echo "==> Could not parse host:port from MIKE_DATABASE_URL in .env." >&2
    echo "==> Set MIKE_DATABASE_URL=postgres://user:pass@host:port/db and re-run." >&2
    exit 1
fi

# TCP probe. On macOS, Local Network privacy can refuse Homebrew bash's /dev/tcp
# ("No route to host") while /usr/bin/nc reaches the same host, so nc is the fallback.
tcp_open() {
    (exec 3<>"/dev/tcp/$1/$2") 2>/dev/null && return 0
    command -v nc >/dev/null 2>&1 && nc -z -w 3 "$1" "$2" >/dev/null 2>&1
}

# The subshell in tcp_open closes the FD on exit. Wrapped in a loop so
# that "retry" below can re-probe without the user having to re-run the script —
# the usual reason the capture DB is missing is its container (or its host) still
# coming up after a reboot.
while :; do
    if tcp_open "${DB_HOST}" "${DB_PORT}"; then
        echo "==> Capture DB at ${DB_HOST}:${DB_PORT} is reachable"
        break
    fi

    # Remote unreachable. If a local fallback DB is configured (and fallback is not
    # explicitly disabled), don't hard-exit — the proxy will fall back to it at boot
    # (see docs/LOCAL_DB_FALLBACK.md). Probe the local URL so we fail fast only when
    # NEITHER target is reachable.
    LOCAL_URL="$(env_value MIKE_LOCAL_DATABASE_URL)"
    LOCAL_URL="${MIKE_LOCAL_DATABASE_URL:-${LOCAL_URL}}"
    FALLBACK_OFF="$(env_value MIKE_DB_FALLBACK_ENABLED)"
    FALLBACK_OFF="${MIKE_DB_FALLBACK_ENABLED:-${FALLBACK_OFF}}"
    LOCAL_OK=false
    if [[ -n "${LOCAL_URL}" && "${FALLBACK_OFF}" != "0" && "${LOCAL_URL}" == *@* ]]; then
        L_HP="${LOCAL_URL#*@}"; L_HP="${L_HP%%/*}"
        L_HOST="${L_HP%%:*}"; L_PORT="${L_HP##*:}"; [[ "${L_HP}" == *:* ]] || L_PORT="5432"
        if tcp_open "${L_HOST}" "${L_PORT}"; then LOCAL_OK=true; fi
    fi
    if ${LOCAL_OK}; then
        echo "==> Remote capture DB at ${DB_HOST}:${DB_PORT} unreachable — proxy will fall back to local DB at ${L_HOST}:${L_PORT} (MIKE_LOCAL_DATABASE_URL)." >&2
        break
    fi

    echo "==> Capture DB at ${DB_HOST}:${DB_PORT} (MIKE_DATABASE_URL) is unreachable." >&2
    if [[ "${DB_HOST}" == "127.0.0.1" || "${DB_HOST}" == "localhost" ]]; then
        echo "==> The capture DB is a container on this machine: scripts/capture-db/up.sh starts it. Then retry." >&2
    else
        echo "==> Bring the database up or fix the connection to ${DB_HOST}, then retry." >&2
    fi
    echo "==> Or configure MIKE_LOCAL_DATABASE_URL (scripts/local-db/up.sh) for local-fallback capture." >&2
    # Without a terminal this takes "a" and exits 1, exactly as it did before.
    DB_ANSWER="$(ask_choice \
        "==> [r] retry the connection  [s] start anyway  [a] abort  (Enter = abort):" \
        "rsa" "a" "s")"
    case "${DB_ANSWER}" in
        r)
            echo "==> Retrying ${DB_HOST}:${DB_PORT} in ${DB_RETRY_PAUSE_SECONDS}s..."
            sleep "${DB_RETRY_PAUSE_SECONDS}"
            ;;
        s)
            echo "==> Starting anyway. The build and smoke test run first, so the database"
            echo "==> has until the smoke test to come back; the deploy fails if it does not."
            break
            ;;
        *)
            exit 1
            ;;
    esac
done

# ── Already running? Say so, and carry on ────────────────────────────────────
# This used to stop and ask "Rebuild and restart it?". Running the script is
# the answer to that question, so it is no longer asked: the lines below only
# report what is up before the build starts, so a DOWN port is seen now rather
# than minutes later. Nothing here costs a healthy proxy anything — Step 2
# leaves a byte-identical one untouched.
#
# Skipped under --check, which starts nothing and so cannot bring a port back.
if [[ "${CHECK_ONLY}" != "true" ]]; then
    # The authoritative ports come from print-config, which needs a built binary
    # we have not made yet, so fall back to the documented defaults here.
    LIVE_INSPECT_ADDR="${MIKE_INSPECT_ADDR:-$(env_value MIKE_INSPECT_ADDR)}"
    LIVE_INSPECT_ADDR="${LIVE_INSPECT_ADDR:-127.0.0.1:9989}"
    LIVE_HEALTH="$(curl --silent --fail --max-time 2 "http://${LIVE_INSPECT_ADDR}/api/health" 2>/dev/null || true)"

    if [[ -n "${LIVE_HEALTH}" ]]; then
        # Tolerate whitespace after the colons: /api/health is compact today, but
        # a pretty-printed payload should degrade to "unknown", not to silence.
        LIVE_SHA="$(printf '%s' "${LIVE_HEALTH}" | sed -n 's/.*"sha_short":[[:space:]]*"\([^"]*\)".*/\1/p')"
        LIVE_UPTIME="$(printf '%s' "${LIVE_HEALTH}" | sed -n 's/.*"uptime_seconds":[[:space:]]*\([0-9][0-9]*\).*/\1/p')"
        LIVE_FOR="${LIVE_UPTIME:-0}"
        if (( LIVE_FOR >= 3600 )); then LIVE_FOR="$(( LIVE_FOR / 3600 ))h"
        elif (( LIVE_FOR >= 60 )); then LIVE_FOR="$(( LIVE_FOR / 60 ))m"
        else LIVE_FOR="${LIVE_FOR}s"; fi

        echo
        echo "==> mike is already running on ${LIVE_INSPECT_ADDR} (build ${LIVE_SHA:-unknown}, up ${LIVE_FOR}) — building, and replacing it only if the build differs."
        for _port_label in "${MIKE_FRONTEND_PORT:-9990}:web UI"; do
            _p="${_port_label%%:*}"; _l="${_port_label#*:}"
            if lsof -ti ":${_p}" -sTCP:LISTEN >/dev/null 2>&1; then
                echo "==>   ${_l} on :${_p} is up."
            else
                echo "==>   ${_l} on :${_p} is DOWN — this run brings it back."
            fi
        done
    fi
fi

# ── Toolchain preflight ───────────────────────────────────────────────────────
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/preflight-toolchain.sh"
ensure_build_toolchain

# ── Step 1: mike-mcp — build always, install only when the binary changed ───
# make install release-builds mike-mcp and copies it to ~/.local/bin, which
# forces a Claude Code restart to pick up the new bridge. The build is
# incremental (cheap when nothing changed); skip the install+restart cost when
# the fresh binary is byte-identical to what's already installed. Runs while the
# old proxy is still serving.
echo "==> Checking mike-mcp (MCP bridge)"
MCP_TARGET_DIR="${CARGO_TARGET_DIR:-${PROJECT_ROOT}/target}"
CARGO_TARGET_DIR="${MCP_TARGET_DIR}" cargo build --release --bin mike-mcp --quiet
MCP_BUILT="${MCP_TARGET_DIR}/release/mike-mcp"
MCP_INSTALLED="${INSTALL_DIR:-${HOME}/.local/bin}/mike-mcp"
if [[ "${FORCE}" == "true" ]] || bins_differ "${MCP_BUILT}" "${MCP_INSTALLED}"; then
    echo "==> mike-mcp changed — installing to ${MCP_INSTALLED} (restart Claude Code to pick it up)"
    make install
else
    echo "==> mike-mcp unchanged — skipping install (no Claude Code restart needed)"
fi

# ── Step 2: deploy-proxy.sh — safe cutover of mike ───────────────────
# Builds mike, runs migrate-check (read-only), smoke-tests the new binary
# on alt ports, then cuts over. Proxy runs DETACHED after this step.
# Old proxy stays untouched until the new one proves it can boot.
echo
if [[ "${CHECK_ONLY}" == "true" ]]; then
    echo "==> deploy-proxy.sh --dry-run --${BUILD_PROFILE} (validation only)"
    "${SCRIPT_DIR}/deploy-proxy.sh" --dry-run "--${BUILD_PROFILE}"
    echo "==> --check: proxy validated. Not starting SPA / Storybook."
    exit 0
else
    FORCE_FLAG=""
    [[ "${FORCE}" == "true" ]] && FORCE_FLAG="--force"
    echo "==> deploy-proxy.sh --${BUILD_PROFILE} ${FORCE_FLAG:-}"
    "${SCRIPT_DIR}/deploy-proxy.sh" "--${BUILD_PROFILE}" ${FORCE_FLAG:+"${FORCE_FLAG}"}
fi

# ── Step 3: resolve ports from the deployed binary ───────────────────────────
# print-config --shell emits the effective MIKE_* exports from the same config
# the deployed binary resolved — authoritative port numbers for the SPA proxy.
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -n1)"
[[ -z "${TARGET_DIR}" ]] && TARGET_DIR="${PROJECT_ROOT}/target"
MIKE_BIN="${TARGET_DIR}/${BUILD_PROFILE}/mike"

eval "$("${MIKE_BIN}" print-config --shell)"

MIKE_ADDR="${MIKE_LISTEN_ADDR}"
INSPECT_ADDR="${MIKE_INSPECT_ADDR}"
export MIKE_FRONTEND_PORT
export MIKE_FRONTEND_HOST="${MIKE_FRONTEND_HOST:-127.0.0.1}"
export INSPECT_BACKEND="${INSPECT_BACKEND:-http://${INSPECT_ADDR}}"

SPA_PORT="${MIKE_FRONTEND_PORT}"
STORYBOOK_PORT="${MIKE_STORYBOOK_PORT:-6006}"

# free_port is provided by lib/proxy-runtime.sh (sourced above).

# ── Pretty summary box (colors + clickable links) ────────────────────────────
# Colors only on a TTY and when NO_COLOR is unset; otherwise everything degrades
# to plain text so piped/captured output stays clean.
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    C_DIM=$'\033[2m'; C_RST=$'\033[0m'; C_BOLD=$'\033[1m'; C_URL=$'\033[36m'; C_WARN=$'\033[1;33m'
    USE_LINKS=1
else
    C_DIM=""; C_RST=""; C_BOLD=""; C_URL=""; C_WARN=""
    USE_LINKS=0
fi

# Cyan, OSC-8 hyperlinked URL (clickable in modern terminals; plain coloured
# text where OSC-8 is unsupported).
hlink() {
    local url="$1"
    if (( USE_LINKS )); then
        printf '%s\033]8;;%s\033\\%s\033]8;;\033\\%s' "${C_URL}" "${url}" "${url}" "${C_RST}"
    else
        printf '%s%s%s' "${C_URL}" "${url}" "${C_RST}"
    fi
}

# Rows are built into two parallel arrays: BOX_PLAIN (for width math — no escape
# codes) and BOX_FANCY (what's actually printed — colours + links). add_row
# pads the label to a fixed column; add_line is a label-less row.
BOX_PLAIN=(); BOX_FANCY=()
add_line()  { BOX_PLAIN+=("$1"); BOX_FANCY+=("$2"); }   # label-less row
add_row() {   # label  plain_value  fancy_value
    BOX_PLAIN+=("$(printf '%-13s%s' "$1" "$2")")
    BOX_FANCY+=("$(printf '%s%-13s%s%s' "${C_DIM}" "$1" "${C_RST}" "$3")")
}

# Draw the rounded box around BOX_PLAIN/BOX_FANCY. Width adapts to the longest
# (escape-free) plain line, so colour codes never throw the border off.
print_box() {
    local width=0 i n=${#BOX_PLAIN[@]} left=2 right=2 inner border pad
    for ((i = 0; i < n; i++)); do (( ${#BOX_PLAIN[i]} > width )) && width=${#BOX_PLAIN[i]}; done
    inner=$(( left + width + right ))
    border="$(printf '─%.0s' $(seq 1 "${inner}"))"
    printf '  %s╭%s╮%s\n' "${C_DIM}" "${border}" "${C_RST}"
    for ((i = 0; i < n; i++)); do
        pad=$(( right + width - ${#BOX_PLAIN[i]} ))
        printf '  %s│%s%*s%s%*s%s│%s\n' \
            "${C_DIM}" "${C_RST}" "${left}" '' "${BOX_FANCY[i]}" "${pad}" '' "${C_DIM}" "${C_RST}"
    done
    printf '  %s╰%s╯%s\n' "${C_DIM}" "${border}" "${C_RST}"
}

# ── What is actually answering ───────────────────────────────────────────────
# The box used to say "Stack is up" without looking. Each service is registered
# here with an address to ask, and asked just before the box is drawn, so a row
# whose service does not answer says so and the heading names it.
#
# STARTED_PORTS holds the ports this run started (under a unit or in the
# foreground); FOREGROUND_PORTS the ones it started in this terminal. Both are
# space-padded strings rather than arrays so an empty one expands safely under
# `set -u` on an old bash.
STARTED_PORTS=" "
FOREGROUND_PORTS=" "
FOREGROUND_LABELS=()
# Twenty seconds covers Storybook, the slowest to bind; nothing is waited for
# when everything already answers.
SUMMARY_WAIT_SECONDS="${START_PROXY_SUMMARY_WAIT:-20}"
SVC_LABELS=(); SVC_URLS=(); SVC_PORTS=(); SVC_STATES=()
add_service() {   # label  url-to-ask  port
    SVC_LABELS+=("$1"); SVC_URLS+=("$2"); SVC_PORTS+=("$3"); SVC_STATES+=("unknown")
}

# True when something answers HTTP at $1. Any reply counts — the sidecar is a
# forward proxy and answers a plain GET with 400 — except on a health route,
# which has to return success.
answers() {
    if [[ "$1" == */health ]]; then
        curl --silent --fail --output /dev/null --max-time 2 "$1" 2>/dev/null
    else
        curl --silent --output /dev/null --max-time 2 "$1" 2>/dev/null
    fi
}

# Ask every registered service, giving one that is still binding a few seconds.
# A service started in this terminal's foreground (no systemd) is not waited
# for: it prints its own "ready" line under the box. Ends with each state set to up, starting (this run started it
# and it has not answered yet) or down (nothing started it now and it is silent).
probe_services() {
    local deadline=$(( SECONDS + SUMMARY_WAIT_SECONDS )) i waiting
    while :; do
        waiting=0
        for ((i = 0; i < ${#SVC_PORTS[@]}; i++)); do
            [[ "${SVC_STATES[i]}" == "up" ]] && continue
            if answers "${SVC_URLS[i]}"; then
                SVC_STATES[i]="up"
            elif [[ "${FOREGROUND_PORTS}" != *" ${SVC_PORTS[i]} "* ]]; then
                waiting=1
            fi
        done
        (( waiting == 0 || SECONDS >= deadline )) && break
        sleep 1
    done
    for ((i = 0; i < ${#SVC_PORTS[@]}; i++)); do
        [[ "${SVC_STATES[i]}" == "up" ]] && continue
        if [[ "${STARTED_PORTS}" == *" ${SVC_PORTS[i]} "* ]]; then
            SVC_STATES[i]="starting"
        else
            SVC_STATES[i]="down"
        fi
    done
}

# The labels of the services in state $1, comma-separated; empty when none.
services_in_state() {
    local i out=""
    for ((i = 0; i < ${#SVC_PORTS[@]}; i++)); do
        [[ "${SVC_STATES[i]}" == "$1" ]] && out+="${out:+, }${SVC_LABELS[i]}"
    done
    printf '%s' "${out}"
}

# A row for the service on port $3, marked when that service is not answering.
# $2 is the plain value and $4 the printed one (links, colour); $4 defaults to
# $2 as a link.
add_service_row() {   # label  plain_value  port  [fancy_value]
    local i note="" fancy_note="" fancy="${4:-$(hlink "$2")}"
    for ((i = 0; i < ${#SVC_PORTS[@]}; i++)); do
        [[ "${SVC_PORTS[i]}" == "$3" ]] || continue
        case "${SVC_STATES[i]}" in
            starting) note="   (starting)";    fancy_note="   ${C_DIM}(starting)${C_RST}" ;;
            down)     note="   NOT ANSWERING"; fancy_note="   ${C_WARN}NOT ANSWERING${C_RST}" ;;
        esac
    done
    add_row "$1" "$2${note}" "${fancy}${fancy_note}"
}

# Install frontend deps on first run.
if [[ ! -d "${PROJECT_ROOT}/frontend/node_modules" ]]; then
    echo "==> Installing frontend dependencies (first run)"
    (cd frontend && pnpm install)
fi

# ── Step 3b: proxy-forward — HTTPS_PROXY MITM sidecar (persistent) ────────────
# The sidecar is the continuity layer. While it stays alive it health-probes the
# routed leg (:9988) and, whenever the proxy is down — e.g. during the cutover in
# Step 2 — routes /v1/* DIRECT to Anthropic instead of failing, then re-attaches
# once the proxy is healthy again. A client in Remote-Control mode (HTTPS_PROXY
# below, ANTHROPIC_BASE_URL unset) therefore never sees a proxy swap. Restarting
# the sidecar here would punch a hole in that very layer, so a running,
# byte-identical sidecar is left untouched; it is only (re)started when its
# binary changed, it is not running, or --force is given.
#
# deploy-proxy.sh has already done exactly that in Step 2 (ensure_proxy_forward,
# on both its no-cutover and its cutover path), so it is not repeated here: a
# second call printed a second, contradictory verdict ("Starting proxy-forward",
# then "leaving it alive") and restarted the sidecar twice under --force.
MIKE_FORWARD_CA="${DEPLOY_DIR}/proxy-forward-ca.pem"
MIKE_FORWARD_CA_REL="${MIKE_FORWARD_CA#"${PROJECT_ROOT}/"}"

# ── Step 3c: prompt-assistant — Claude Agent SDK service (persistent) ────────
# services/prompt-assistant/ is a small standalone Node/TypeScript service
# (its own package.json, not part of the frontend pnpm workspace) that the SPA
# reaches at /api/assistant via vite's dev proxy. Like proxy-forward it runs
# detached and outlives this script (Ctrl-C stops only what runs in this
# terminal's foreground) — a running instance is left alone unless it
# is down, its build output has changed since it was started, or --force is
# given (G16: a prior version of this script left a running instance alone
# unconditionally, so a changed services/prompt-assistant/ kept serving a
# stale dist/ until someone remembered --force).
PROMPT_ASSISTANT_PORT="${PROMPT_ASSISTANT_PORT:-$(env_value PROMPT_ASSISTANT_PORT)}"
PROMPT_ASSISTANT_PORT="${PROMPT_ASSISTANT_PORT:-9985}"

# Content hash of every built .js file under services/prompt-assistant/dist,
# combined into one hash — mirrors bin_sha/bins_differ's "compare content, not
# mtimes" philosophy (proxy-runtime.sh) for a multi-file build output instead
# of a single binary. Empty string when dist/ is missing or empty, so a first
# build never spuriously compares equal to "no stamp yet".
#
# The file list is taken relative to dist/. Hashed with absolute paths, the
# result changed with the path the checkout was reached by (/home/.../projects
# is a symlink to /mnt/data/projects here), so a run from the other path saw
# "stale build output" and restarted an assistant that had not changed.
prompt_assistant_dist_sha() {
    local dist_dir="$1/dist"
    [[ -d "${dist_dir}" ]] || { echo ""; return 0; }
    (
        cd "${dist_dir}" || exit 0
        files="$(find . -type f -name '*.js' | sort)"
        [[ -z "${files}" ]] && exit 0
        printf '%s\n' "${files}" | xargs bash -c 'source "$0" && sha256 "$@"' "${SCRIPT_DIR}/lib/sha256.sh" | sha256
    )
}

ensure_prompt_assistant() {
    local dir="${PROJECT_ROOT}/services/prompt-assistant"
    local log="${DEPLOY_DIR}/prompt-assistant.log"
    local stamp="${DEPLOY_DIR}/prompt-assistant.dist-sha"
    echo
    echo "==> Checking prompt-assistant (Claude Agent SDK service)"
    if [[ ! -d "${dir}" ]]; then
        echo "==> services/prompt-assistant not present — skipping" >&2
        return 0
    fi

    local already_running=false under_launchd=false
    if managed_service_installed && [[ "$(managed_service_platform)" == "launchd" ]]; then
        under_launchd=true
    fi
    if ${under_launchd}; then
        [[ -n "$(managed_service_pid assistant)" ]] && already_running=true
    elif stack_units_enabled; then
        unit_active mike-stack-assistant && already_running=true
    else
        lsof -ti ":${PROMPT_ASSISTANT_PORT}" -sTCP:LISTEN >/dev/null 2>&1 && already_running=true
    fi

    echo "==> Building prompt-assistant"
    if ! ( cd "${dir}" && pnpm install --prefer-offline --silent && pnpm --silent run build ); then
        echo "==> prompt-assistant build failed — leaving any running instance untouched" >&2
        return 0
    fi

    local new_sha old_sha=""
    new_sha="$(prompt_assistant_dist_sha "${dir}")"
    [[ -f "${stamp}" ]] && old_sha="$(cat "${stamp}" 2>/dev/null || true)"

    if [[ "${FORCE}" != "true" ]] && ${already_running} && [[ -n "${new_sha}" && "${new_sha}" == "${old_sha}" ]]; then
        echo "==> prompt-assistant already running on :${PROMPT_ASSISTANT_PORT} and up to date — leaving it alone"
        return 0
    fi
    if ${already_running}; then
        local why="stale build output"
        [[ "${FORCE}" == "true" ]] && why="--force"
        echo "==> prompt-assistant on :${PROMPT_ASSISTANT_PORT} needs restarting (${why})"
    fi

    if ${under_launchd}; then
        managed_service_stop assistant || true
        free_port "${PROMPT_ASSISTANT_PORT}"
        install_assistant_agent "$(cd "${PROJECT_ROOT}" && pwd -P)" "${PROMPT_ASSISTANT_PORT}" "${INSPECT_ADDR}"
        [[ -s "${log}" ]] && mv -f "${log}" "${log}.1"
        echo "==> Starting prompt-assistant under $(managed_service_identifier assistant) (log -> ${log#"${PROJECT_ROOT}/"})"
        managed_service_restart assistant
        STARTED_PORTS+="${PROMPT_ASSISTANT_PORT} "
        printf '%s' "${new_sha}" > "${stamp}" 2>/dev/null || true
        return 0
    fi
    if stack_units_enabled; then
        install_stack_units
        systemctl --user stop mike-stack-assistant.service 2>/dev/null || true
        free_port "${PROMPT_ASSISTANT_PORT}"
        [[ -s "${log}" ]] && mv -f "${log}" "${log}.1"
        echo "==> Starting prompt-assistant under mike-stack-assistant (log -> ${log#"${PROJECT_ROOT}/"})"
        unit_start mike-stack-assistant
        STARTED_PORTS+="${PROMPT_ASSISTANT_PORT} "
        printf '%s' "${new_sha}" > "${stamp}" 2>/dev/null || true
        return 0
    fi
    free_port "${PROMPT_ASSISTANT_PORT}"
    STARTED_PORTS+="${PROMPT_ASSISTANT_PORT} "
    echo "==> Starting prompt-assistant on :${PROMPT_ASSISTANT_PORT} (detached; log -> ${log#"${PROJECT_ROOT}/"})"
    mkdir -p "${DEPLOY_DIR}"
    [[ -s "${log}" ]] && mv -f "${log}" "${log}.1"
    # env -u: matches proxy-forward/mike-mcp — an inherited session tag would let
    # this session's SessionEnd reap a service every session shares. The rest
    # (G2) are the parent Claude Code session's own vars: the agent this
    # service spawns must never inherit its effort override, its child-session
    # marker, or its remote-control bridge/messaging channels — runTurn.ts's
    # own env allowlist is the primary guard, this is the belt on top of it.
    ( cd "${dir}" && \
        PROMPT_ASSISTANT_PORT="${PROMPT_ASSISTANT_PORT}" \
        INSPECT_URL="http://${INSPECT_ADDR}" \
        nohup env \
            -u CLAUDE_CODE_SESSION_ID \
            -u CLAUDE_EFFORT \
            -u CLAUDE_CODE_CHILD_SESSION \
            -u CLAUDE_CODE_BRIDGE_SESSION_ID \
            -u CLAUDE_CODE_MESSAGING_SOCKET \
            -u CLAUDE_CODE_MESSAGING_TOKEN \
            -u MAX_THINKING_TOKENS \
            node dist/server.js > "${log}" 2>&1 & )
    disown 2>/dev/null || true
    printf '%s' "${new_sha}" > "${stamp}" 2>/dev/null || true
}
ensure_prompt_assistant

# ── SPA + Storybook ──────────────────────────────────────────────────────────
# The Rust proxy is already running detached (managed by deploy-proxy.sh). vite
# dev is HMR, so an already-running frontend needs no "redeploy" — it picks up
# source changes live. This run therefore LEAVES a running SPA/Storybook alone
# (like the sidecar) and only starts one that is down, or everything under
# --force. Under systemd both are units and nothing below runs in the
# foreground. Without it they do, and the cleanup trap tears down ONLY what THIS
# run started, so a frontend we left alive (e.g. another terminal's supervisor)
# survives our exit.
PIDS=()
LAUNCHED_PORTS=()
cleanup() {
    local exit_code=$?
    if (( ${#PIDS[@]} )); then
        echo
        echo "==> Stopping what this run started in this terminal (the rest of the stack keeps running)"
        for pid in "${PIDS[@]}"; do
            kill "${pid}" 2>/dev/null || true
        done
        wait "${PIDS[@]}" 2>/dev/null || true
    fi
    if (( ${#LAUNCHED_PORTS[@]} )); then
        for port in "${LAUNCHED_PORTS[@]}"; do
            free_port "${port}"
        done
    fi
    exit "${exit_code}"
}
trap cleanup EXIT INT TERM

# (Re)launch one frontend port only when it is down, or under --force. An
# already-listening port is left running so `start-proxy.sh` as a redeploy never
# drops a live dev server.
maybe_start_frontend() {   # $1=port  $2=label  $3=cmd (run via bash -c in frontend/)
    local port="$1" label="$2" cmd="$3"
    if [[ "${FORCE}" != "true" ]] && lsof -ti ":${port}" -sTCP:LISTEN >/dev/null 2>&1; then
        echo "==> ${label} already running on :${port} — leaving it (HMR picks up changes live)"
        return 0
    fi
    free_port "${port}"
    echo "==> Starting ${label}"
    # env -u: these serve every session on the box, so an inherited session tag
    # would let one session's SessionEnd reap the shared dev server.
    ( cd "${PROJECT_ROOT}/frontend" && exec env -u CLAUDE_CODE_SESSION_ID bash -c "${cmd}" ) &
    PIDS+=($!)
    LAUNCHED_PORTS+=("${port}")
    STARTED_PORTS+="${port} "
    FOREGROUND_PORTS+="${port} "
    FOREGROUND_LABELS+=("${label%% (*}")
}

if managed_service_installed; then
    # Under an installed service deploy-proxy.sh has deployed through it, and the web UI on this
    # port is that service's own host, not vite. Freeing the port (maybe_start_frontend does under
    # --force) would stop the proxy itself.
    SPA_LOG_REL=""
    echo "==> The web UI on :${SPA_PORT} is served by the installed service ($(managed_service_identifier proxy)) — not starting vite, leaving it alone"
elif stack_units_enabled && [[ "${SPA_PORT}" == "9990" ]]; then
    install_stack_units
    if [[ "${FORCE}" == "true" ]] || ! unit_active mike-stack-spa; then
        systemctl --user stop mike-stack-spa.service 2>/dev/null || true
        free_port "${SPA_PORT}"
        echo "==> Starting SPA host under mike-stack-spa (log -> .deploy/spa.log)"
        unit_start mike-stack-spa
        STARTED_PORTS+="${SPA_PORT} "
    else
        echo "==> SPA host running under mike-stack-spa — leaving it (HMR picks up changes live)"
    fi
    SPA_LOG_REL=".deploy/spa.log"
else
    SPA_LOG_REL=""
    maybe_start_frontend "${SPA_PORT}"       "SPA host (vite dev — HMR enabled)" "pnpm run start"
fi
# Storybook and the architecture diagrams are not started by a deploy. They are
# units to start by hand: systemctl --user start mike-stack-storybook mike-stack-arch.
# Their rows show only when they answer.
STORYBOOK_LOG_REL=""
ARCH_LOG_REL=""
ARCH_PORT="${ARCH_PORT:-$(env_value ARCH_PORT)}"
ARCH_PORT="${ARCH_PORT:-9986}"

# ── Summary box: every URL + log location in one place ───────────────────────
# Relative log paths (script runs from repo root) keep the box compact.
MIKE_LOG_REL="${MIKE_LOG#"${PROJECT_ROOT}/"}"

LAN_IP=""
if [[ "${MIKE_FRONTEND_HOST}" == "0.0.0.0" ]]; then
    # macOS first, then Linux; no row at all when neither can say.
    LAN_IP="$(ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1 2>/dev/null \
        || ip -4 -o route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' || true)"
    WEB_URL="http://127.0.0.1:${SPA_PORT}/"
else
    WEB_URL="http://${MIKE_FRONTEND_HOST}:${SPA_PORT}/"
fi

# The style guide the box points at is the one drawn with the app's real
# components: the Foundations/Style Guide stories in Storybook. It used to link
# the bundled export docs/frontend/STYLE_GUIDE.html, which is a picture of the
# guide taken at export time and goes out of date between exports.
STORYBOOK_URL="http://127.0.0.1:${STORYBOOK_PORT}/"
STYLE_GUIDE_URL="${STORYBOOK_URL}?path=/story/foundations-style-guide--core"
INSPECT_PORT="${INSPECT_ADDR##*:}"
ASSISTANT_URL="http://127.0.0.1:${PROMPT_ASSISTANT_PORT}/api/assistant/health"
SIDECAR_URL="http://127.0.0.1:${MIKE_FORWARD_PORT}"

add_service "proxy"            "http://${INSPECT_ADDR}/api/health" "${INSPECT_PORT}"
add_service "sidecar"          "${SIDECAR_URL}/"                   "${MIKE_FORWARD_PORT}"
add_service "prompt assistant" "${ASSISTANT_URL}"                  "${PROMPT_ASSISTANT_PORT}"
add_service "web UI"           "${WEB_URL}"                        "${SPA_PORT}"
probe_services

DOWN_NOW="$(services_in_state down)"
STARTING_NOW="$(services_in_state starting)"
if [[ -n "${DOWN_NOW}" ]]; then
    HEADING="Stack is NOT fully up — not answering: ${DOWN_NOW}"
    add_line "${HEADING}" "${C_WARN}${HEADING}${C_RST}"
elif [[ -n "${STARTING_NOW}" ]]; then
    HEADING="Stack is up — still starting: ${STARTING_NOW}"
    add_line "${HEADING}" "${C_BOLD}${HEADING}${C_RST}"
else
    add_line "Stack is up" "${C_BOLD}Stack is up${C_RST}"
fi

add_service_row "Web UI:"      "${WEB_URL}"                          "${SPA_PORT}"
[[ -n "${LAN_IP}" ]] && add_row "  (LAN):" "http://${LAN_IP}:${SPA_PORT}/" "$(hlink "http://${LAN_IP}:${SPA_PORT}/")"
add_service_row "Settings:"    "${WEB_URL}settings"                  "${SPA_PORT}"
if lsof -ti ":${STORYBOOK_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    add_row "Storybook:"   "${STORYBOOK_URL}"   "$(hlink "${STORYBOOK_URL}")"
    add_row "Style guide:" "${STYLE_GUIDE_URL}" "$(hlink "${STYLE_GUIDE_URL}")"
fi
if lsof -ti ":${ARCH_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    add_row "Diagrams:"    "http://127.0.0.1:${ARCH_PORT}/" "$(hlink "http://127.0.0.1:${ARCH_PORT}/")"
fi
add_service_row "REST API:"    "http://${INSPECT_ADDR}/api/requests" "${INSPECT_PORT}"
add_service_row "Assistant:"   "${ASSISTANT_URL}"                    "${PROMPT_ASSISTANT_PORT}"

# How to start a session against this stack. Through the sidecar is the normal
# way: it is captured, and it keeps flowing straight to Anthropic for the
# seconds a proxy swap takes. `cc` is that route wrapped up; the row under it is
# the same thing spelled out, which needs the sidecar's CA or TLS fails.
if [[ -f "${PROJECT_ROOT}/scripts/shell/cc.zsh" ]]; then
    add_service_row "Sessions:" "cc   (scripts/shell/cc.zsh: through the sidecar, rides out a proxy swap)" "${MIKE_FORWARD_PORT}" \
        "${C_BOLD}cc${C_RST}   ${C_DIM}(scripts/shell/cc.zsh: through the sidecar, rides out a proxy swap)${C_RST}"
    BY_HAND_LABEL="  by hand:"
else
    BY_HAND_LABEL="Sessions:"
fi
add_service_row "${BY_HAND_LABEL}" "NODE_EXTRA_CA_CERTS=${MIKE_FORWARD_CA_REL} HTTPS_PROXY=${SIDECAR_URL} claude" "${MIKE_FORWARD_PORT}" \
    "NODE_EXTRA_CA_CERTS=${MIKE_FORWARD_CA_REL} HTTPS_PROXY=$(hlink "${SIDECAR_URL}") claude"
add_service_row "  direct:" "ANTHROPIC_BASE_URL=http://${MIKE_ADDR} claude   (feels a proxy swap)" "${INSPECT_PORT}" \
    "ANTHROPIC_BASE_URL=$(hlink "http://${MIKE_ADDR}") claude   ${C_DIM}(feels a proxy swap)${C_RST}"

# One row for the logs: every service that writes one, as a path the shell expands.
SERVICE_INSTALLED=false
managed_service_installed && SERVICE_INSTALLED=true
LOG_NAMES="mike,smoke,proxy-forward,prompt-assistant"
${SERVICE_INSTALLED} && LOG_NAMES="smoke,prompt-assistant"
[[ -n "${SPA_LOG_REL}" ]] && LOG_NAMES+=",spa"
[[ -n "${STORYBOOK_LOG_REL}" ]] && LOG_NAMES+=",storybook"
[[ -n "${ARCH_LOG_REL}" ]] && LOG_NAMES+=",arch"
add_row "Logs:"       ".deploy/{${LOG_NAMES}}.log"  ".deploy/{${LOG_NAMES}}.log"
if ${SERVICE_INSTALLED}; then
    SERVICE_LOG_ROW="$(managed_service_log || true)"
    [[ "${SERVICE_LOG_ROW}" == /* ]] && SERVICE_LOG_ROW="tail -f ${SERVICE_LOG_ROW}"
    add_row "Tail proxy:" "${SERVICE_LOG_ROW}" "${SERVICE_LOG_ROW}"
    add_row "Status:"  "mike service status"  "mike service status"
    add_row "Restart:" "mike service restart" "mike service restart"
else
    add_row "Tail proxy:" "tail -f ${MIKE_LOG_REL}"    "tail -f ${MIKE_LOG_REL}"
    if stack_units_enabled; then
        add_row "Status:" "systemctl --user status 'mike-stack-*'" "systemctl --user status 'mike-stack-*'"
    fi
    add_row "Stop:"       "./scripts/stop-proxy.sh"     "./scripts/stop-proxy.sh"
fi

echo
print_box
if [[ -n "${DOWN_NOW}" ]]; then
    printf '  %sNot answering: %s. Look at its log above, or the unit status, before relying on the stack.%s\n' "${C_WARN}" "${DOWN_NOW}" "${C_RST}"
fi
if (( ${#PIDS[@]} )); then
    FOREGROUND_NAMES="$(IFS=,; printf '%s' "${FOREGROUND_LABELS[*]}")"
    printf '  %sCtrl-C stops what this run started in this terminal (%s); everything else keeps running.%s\n\n' \
        "${C_DIM}" "${FOREGROUND_NAMES//,/, }" "${C_RST}"
    wait "${PIDS[@]}"
else
    # Nothing was started in the foreground, so there is nothing to supervise.
    # Don't block the terminal — the trap kills nothing (PIDS empty), so what
    # was already running keeps serving. Whether the proxy was replaced or left
    # alone is deploy-proxy.sh's line above, not something to restate here.
    if stack_units_enabled; then
        printf '  %sEverything runs under systemd (mike-stack-*) and keeps running after this script exits.%s\n\n' "${C_DIM}" "${C_RST}"
    else
        printf '  %sNothing was started in this terminal, so there is nothing to supervise — exiting. The stack keeps running.%s\n\n' "${C_DIM}" "${C_RST}"
    fi
fi
