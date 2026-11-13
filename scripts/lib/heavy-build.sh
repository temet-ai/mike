#!/usr/bin/env bash
# heavy-build.sh — keeps the builds of every session on a machine from piling up.
#
# On 2026-11-13 a deploy's release build and several sessions' checks ran at once: load 16 on 16
# cores, the CPU at 83°C and 12 GB in swap. So every heavy build that scripts/cx and
# scripts/deploy-proxy.sh run (check, clippy, build, test, nextest):
#   - runs at the lowest priority: nice -n 19, and ionice -c 3 where it exists (Linux);
#   - uses at most half the processors (MIKE_BUILD_JOBS overrides, CARGO_BUILD_JOBS when set);
#   - holds the machine's build lock while it runs, so a second build waits instead of competing.
#     The lock is an flock on a file under $XDG_RUNTIME_DIR (else $TMPDIR, else /tmp) on file
#     descriptor 8; it is released when the holding shell exits however it exits. Where there is
#     no flock command (macOS without util-linux) a directory lock stands in, with the holder's
#     pid so a dead holder's lock is taken over. A nested call (cx inside sqlx-prepare inside cx)
#     sees MIKE_HEAVY_BUILD_LOCK_HELD and does not wait on itself.
# Commands run under the lock must close descriptor 8 (`8>&-`), or a daemon they start (an
# sccache server) would hold the lock after the build ends.
#
# fmt, metadata and every other cargo call take no lock and keep their priority.

HEAVY_BUILD_SUBCOMMANDS=" check clippy build test nextest "

# heavy_build_is_heavy <subcommand>: 0 for a subcommand that compiles.
heavy_build_is_heavy() {
    [[ "${HEAVY_BUILD_SUBCOMMANDS}" == *" ${1:-} "* ]]
}

heavy_build_cpus() {
    local n
    n="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
    [[ "${n}" =~ ^[0-9]+$ ]] || n=2
    printf '%s\n' "${n}"
}

# heavy_build_jobs: MIKE_BUILD_JOBS, else CARGO_BUILD_JOBS, else half the processors (at least 1).
heavy_build_jobs() {
    if [[ -n "${MIKE_BUILD_JOBS:-}" ]]; then
        printf '%s\n' "${MIKE_BUILD_JOBS}"
    elif [[ -n "${CARGO_BUILD_JOBS:-}" ]]; then
        printf '%s\n' "${CARGO_BUILD_JOBS}"
    else
        local half=$(( $(heavy_build_cpus) / 2 ))
        printf '%s\n' "$(( half > 0 ? half : 1 ))"
    fi
}

# heavy_build_priority: the command prefix a heavy build runs under, one word per line.
heavy_build_priority() {
    printf '%s\n' nice -n 19
    if command -v ionice >/dev/null 2>&1; then
        printf '%s\n' ionice -c 3
    fi
}

heavy_build_lock_path() {
    local dir="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}"
    printf '%s\n' "${MIKE_HEAVY_BUILD_LOCK:-${dir%/}/mike-heavy-build.lock}"
}

heavy_build_lock_mode() {
    if [[ "${MIKE_HEAVY_BUILD_LOCK_MODE:-}" == mkdir ]] || ! command -v flock >/dev/null 2>&1; then
        echo mkdir
    else
        echo flock
    fi
}

# heavy_build_lock <label>: waits for and takes the machine's build lock. A no-op when this
# process tree already holds it. Says on stderr whom it waits for.
heavy_build_lock() {
    local label="$1" path holder
    [[ -n "${MIKE_HEAVY_BUILD_LOCK_HELD:-}" ]] && return 0
    path="$(heavy_build_lock_path)"
    if [[ "$(heavy_build_lock_mode)" == flock ]]; then
        exec 8>>"${path}" || return 1
        if ! flock -n 8; then
            holder="$(cat "${path}" 2>/dev/null || true)"
            echo "${label}: another heavy build is running (${holder:-unknown}); waiting for it — one at a time on this machine" >&2
            flock 8 || return 1
        fi
        printf 'pid %s, %s, since %s\n' "$$" "${label}" "$(date '+%H:%M:%S')" >"${path}"
        HEAVY_BUILD_LOCK_FD=8
    else
        local dir="${path}.d" said=0 pid
        while ! mkdir "${dir}" 2>/dev/null; do
            pid="$(cat "${dir}/pid" 2>/dev/null || true)"
            if [[ -n "${pid}" ]] && ! kill -0 "${pid}" 2>/dev/null; then
                rm -rf "${dir}"
                continue
            fi
            if (( ! said )); then
                echo "${label}: another heavy build is running ($(cat "${dir}/holder" 2>/dev/null || echo unknown)); waiting for it — one at a time on this machine" >&2
                said=1
            fi
            sleep 2
        done
        printf '%s\n' "$$" >"${dir}/pid"
        printf 'pid %s, %s, since %s\n' "$$" "${label}" "$(date '+%H:%M:%S')" >"${dir}/holder"
        HEAVY_BUILD_LOCK_DIR="${dir}"
    fi
    export MIKE_HEAVY_BUILD_LOCK_HELD=1
}

# heavy_build_unlock: releases a lock this shell took. Safe to call when none is held.
heavy_build_unlock() {
    if [[ -n "${HEAVY_BUILD_LOCK_DIR:-}" ]]; then
        rm -rf "${HEAVY_BUILD_LOCK_DIR}"
        HEAVY_BUILD_LOCK_DIR=""
        unset MIKE_HEAVY_BUILD_LOCK_HELD
    elif [[ -n "${HEAVY_BUILD_LOCK_FD:-}" ]]; then
        exec 8>&-
        HEAVY_BUILD_LOCK_FD=""
        unset MIKE_HEAVY_BUILD_LOCK_HELD
    fi
}
