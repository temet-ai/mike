#!/usr/bin/env bash
# proxy-runtime.sh — shared runtime helpers for start-proxy.sh and deploy-proxy.sh.
#
# Single source of truth for the process/port/binary/sidecar management both
# launchers need, so their behaviour cannot silently drift (it used to: two
# free_port copies, a bins_differ-vs-bin_sha split, and two hand-synced copies of
# the proxy-forward sidecar block — the latter grew a real bug on one side).
#
# Source AFTER these globals are set (the functions read them at call time):
#   PROJECT_ROOT, TARGET_DIR, BUILD_PROFILE, RELEASE_FLAG, FORCE
# ensure_proxy_forward exports MIKE_FORWARD_CA (the path the caller prints in its
# Remote-Control hint) and honours MIKE_FORWARD_PORT (default 9987).

# Kill whatever is bound to a TCP port (LISTEN), idempotently, announcing what it
# stops so both scripts' output shows which process/port was freed.
free_port() {
    local port="$1" pid cmd
    pid="$(lsof -ti ":${port}" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -z "${pid}" ]] && return 0
    cmd="$(ps -p "${pid}" -o comm= 2>/dev/null | tr -d ' ' || echo unknown)"
    echo "==> Port ${port} held by ${cmd} (pid ${pid}) — stopping it"
    kill "${pid}" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
        sleep 0.2
        kill -0 "${pid}" 2>/dev/null || return 0
    done
    kill -9 "${pid}" 2>/dev/null || true
    sleep 0.2
}

# Pending-migration count from `mike migrate-check` output on stdin ("Migration
# assessment — N pending, ..."). Prints the count; unreadable output prints 1, so a
# caller that must not migrate treats it as pending (#671).
pending_migration_count() {
    local n
    n="$(awk '/^Migration assessment/ { for (i = 1; i < NF; i++) if ($(i+1) ~ /^pending/) { print $i; exit } }')"
    [[ "${n}" =~ ^[0-9]+$ ]] && echo "${n}" || echo 1
}

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/sha256.sh"

# Content hash of a binary. Empty string when the file is absent,
# so a missing file never compares equal to another missing file.
bin_sha() {
    [[ -f "$1" ]] || { echo ""; return 0; }
    sha256 < "$1"
}

# True (0) when binary $1 differs from stamp $2. A missing stamp counts as
# "differs" so a first-ever deploy always (re)installs.
bins_differ() {
    local a b
    a="$(bin_sha "$1")"
    b="$(bin_sha "$2")"
    [[ -z "${b}" || "${a}" != "${b}" ]]
}

# Ensure the proxy-forward MITM sidecar is running and current. It is the
# continuity layer: while alive it routes /v1/* DIRECT to Anthropic whenever the
# proxy is down — including a deploy's cutover window — so a Remote-Control client
# (HTTPS_PROXY=http://127.0.0.1:<port>, ANTHROPIC_BASE_URL unset) never sees the
# swap. Restarting it punches a hole in that very layer, so:
#   - unchanged binary + already running + not --force -> LEFT ALIVE (no gap)
#   - not running                                       -> started
#   - stale binary, or FORCE=true                       -> replaced + re-stamped
# Best-effort: a sidecar build failure is logged and returns 0 (never aborts the
# caller's proxy deploy).
MIKE_FORWARD_PORT="${MIKE_FORWARD_PORT:-9987}"
ensure_proxy_forward() {
    local deploy_dir="${PROJECT_ROOT}/.deploy"
    mkdir -p "${deploy_dir}"
    local pf_bin="${TARGET_DIR}/${BUILD_PROFILE}/proxy-forward"
    local pf_log="${deploy_dir}/proxy-forward.log"
    local pf_stamp="${deploy_dir}/proxy-forward.deployed"
    MIKE_FORWARD_CA="${deploy_dir}/proxy-forward-ca.pem"   # exported for the caller's summary

    echo "==> Checking proxy-forward (MITM sidecar)"
    if ! cargo build ${RELEASE_FLAG} --manifest-path "${PROJECT_ROOT}/crates/proxy-forward/Cargo.toml" --quiet; then
        echo "==> proxy-forward build failed — leaving any running sidecar untouched" >&2
        return 0
    fi

    local pf_running=false
    lsof -ti ":${MIKE_FORWARD_PORT}" -sTCP:LISTEN >/dev/null 2>&1 && pf_running=true

    if declare -F stack_units_enabled >/dev/null && stack_units_enabled; then
        install_stack_units
        "${pf_bin}" export-ca > "${MIKE_FORWARD_CA}" 2>/dev/null || true
        if [[ "${FORCE}" != "true" ]] && unit_active mike-stack-forward && ! bins_differ "${pf_bin}" "${pf_stamp}"; then
            echo "==> proxy-forward unchanged and running under mike-stack-forward — leaving it alive (no failover gap)"
            return 0
        fi
        systemctl --user stop mike-stack-forward.service 2>/dev/null || true
        free_port "${MIKE_FORWARD_PORT}"
        rm -f "${pf_stamp}"
        cp "${pf_bin}" "${pf_stamp}"
        echo "==> Starting proxy-forward under mike-stack-forward (log -> ${pf_log})"
        [[ -s "${pf_log}" ]] && mv -f "${pf_log}" "${pf_log}.1"
        unit_start mike-stack-forward
        return 0
    fi

    if [[ "${FORCE}" != "true" ]] && ${pf_running} && ! bins_differ "${pf_bin}" "${pf_stamp}"; then
        echo "==> proxy-forward unchanged and already running on :${MIKE_FORWARD_PORT} — leaving the sidecar alive (no failover gap)"
        return 0
    fi

    if ${pf_running}; then
        local why="stale binary"
        [[ "${FORCE}" == "true" ]] && why="--force"
        echo "==> proxy-forward on :${MIKE_FORWARD_PORT} needs replacing (${why}) — restarting the sidecar"
    else
        echo "==> proxy-forward not running — starting the sidecar on :${MIKE_FORWARD_PORT}"
    fi

    free_port "${MIKE_FORWARD_PORT}"
    "${pf_bin}" export-ca > "${MIKE_FORWARD_CA}" 2>/dev/null || true
    echo "==> Starting proxy-forward (detached; log -> ${pf_log})"
    # Rotate rather than truncate — same forensics reason as the proxy log.
    [[ -s "${pf_log}" ]] && mv -f "${pf_log}" "${pf_log}.1"
    # env -u: an inherited session tag lets the SessionEnd reaper kill this shared daemon.
    # 9>&-: deploy-proxy.sh holds its one-deploy-at-a-time lock on fd 9; a sidecar
    # that inherited it would keep the lock for as long as it lives.
    nohup env -u CLAUDE_CODE_SESSION_ID "${pf_bin}" serve --bind "127.0.0.1:${MIKE_FORWARD_PORT}" > "${pf_log}" 2>&1 9>&- &
    disown $! 2>/dev/null || true
    # Stamp what we started (rm-first: macOS running-binary guard) so the next run
    # can detect an unchanged sidecar and leave it alive.
    rm -f "${pf_stamp}"
    cp "${pf_bin}" "${pf_stamp}"
}

