#!/usr/bin/env bash
# heavy-build.sh — keeps the builds of every session on a machine from piling up.
#
# On 2026-11-13 a deploy's release build and several sessions' checks ran at once: load 16 on 16
# cores, the CPU at 83°C and 12 GB in swap. So every heavy build that scripts/cx and
# scripts/deploy-proxy.sh run (check, clippy, build, test, nextest):
#   - runs at the lowest priority: nice -n 19, and ionice -c 3 where it exists (Linux);
#   - on Linux with a systemd user manager, runs in its own scope under background.slice
#     (MIKE_BUILD_SLICE names another slice; empty turns it off). nice and ionice only rank
#     a build against its own cgroup, so without this it competed on equal terms with the
#     desktop apps beside it in app.slice; the slice's CPU, I/O and memory limits are what
#     make it yield (2026-11-16);
#   - uses at most half the processors (MIKE_BUILD_JOBS overrides, CARGO_BUILD_JOBS when set).
# Builds run side by side; the slice and the job cap keep them from starving the machine, and
# mbx shares compiled units between them. fmt, metadata and every other cargo call keep their
# priority.

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

# heavy_build_slice_prefix: `systemd-run --user --scope` into MIKE_BUILD_SLICE (default
# background.slice), one word per line, or nothing: off Linux, without a reachable user manager
# (ssh without a session, CI, containers), or when this process is already in that slice.
heavy_build_slice_prefix() {
    local slice="${MIKE_BUILD_SLICE-background.slice}"
    [[ -n "${slice}" && "$(uname -s)" == Linux ]] || return 0
    command -v systemd-run >/dev/null 2>&1 || return 0
    [[ -n "${XDG_RUNTIME_DIR:-}" && -S "${XDG_RUNTIME_DIR}/systemd/private" ]] || return 0
    grep -q "/${slice}/" /proc/self/cgroup 2>/dev/null && return 0
    printf '%s\n' systemd-run --user --scope --quiet "--slice=${slice}" --
}

# heavy_build_priority: the command prefix a heavy build runs under, one word per line.
heavy_build_priority() {
    heavy_build_slice_prefix
    printf '%s\n' nice -n 19
    if command -v ionice >/dev/null 2>&1; then
        printf '%s\n' ionice -c 3
    fi
}

