#!/usr/bin/env bash
# target-dir-guard.sh — coordination between Rust builds and the sweeper that
# keeps the shared build directory under its size cap.
#
# WHY THIS EXISTS
#   Every session on this machine compiles into one shared directory
#   (CARGO_TARGET_DIR, normally <checkout>/target). Cargo never removes
#   anything from it, so the artifacts of every dependency version ever built
#   pile up: on 2026-08-28 updating 154 crates plus two full checks added 32GB
#   in forty minutes and took the volume to 157MB free, which stops git, the
#   capture database and every build at once.
#
#   The cleaner (~/.local/bin/cargo-target-sweep.sh) evicts oldest-first until
#   the directory is under its cap. Running it while a build is in flight
#   deletes fingerprint directories out from under that build, which is how a
#   check run died on 2026-08-25 with "failed to write .fingerprint/<crate>/
#   invoked.timestamp". Its previous defence was to look for a running rustc
#   process, which both misses a build between two compiler invocations and
#   blocks the sweep forever on a machine that always has a build running.
#
#   So builds and sweeps take turns explicitly. Builds register themselves;
#   a sweep waits for the registrations to drain and blocks new ones while it
#   works. It is a readers-writer arrangement: any number of builds together,
#   or one sweep, never both.
#
# USAGE
#   source scripts/lib/target-dir-guard.sh
#   guard_free_gb                 # free GB on the volume holding the build dir
#   guard_build_enter             # register a build (waits out a running sweep)
#   guard_build_leave             # deregister (also runs from an EXIT trap)
#   guard_sweep_enter             # claim the sweep slot; non-zero if unavailable
#   guard_sweep_leave             # release it
#
#   Never hold a build registration while invoking the sweeper: the sweep waits
#   for builds to drain and the build waits for the sweep, and neither returns.
#   Sweep first, then register.

# The directory holding the turn-taking state. Inside the build directory so it
# travels with it and a wiped build directory cannot leave a stale claim behind.
guard_dir() {
  local root="${CARGO_TARGET_DIR:-$PWD/target}"
  printf '%s/.guard' "$root"
}

# Free space, in whole gigabytes, on the volume the build directory lives on.
# `df` answers immediately at any directory size, unlike measuring the directory
# itself, which is slowest exactly when the directory is biggest.
guard_free_gb() {
  local root="${CARGO_TARGET_DIR:-$PWD/target}"
  [[ -d "$root" ]] || root="$(dirname "$root")"
  # POSIX -Pk works on Linux and macOS; BSD-only -g fails silently under pipefail.
  df -Pk "$root" 2>/dev/null | awk 'NR==2 {printf "%d\n", $4 / 1048576}'
}

# Drop registrations whose process is gone, so a build killed with SIGKILL
# cannot block every future sweep.
# `find` rather than a `build.*` glob: with no registrations the glob expands to
# itself under bash and aborts the function outright under zsh, and this file is
# read by both.
guard_list_builds() {
  find "$(guard_dir)" -maxdepth 1 -name 'build.*' -type f 2>/dev/null
}

guard_prune_stale_builds() {
  local f pid
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    pid="${f##*/build.}"
    kill -0 "$pid" 2>/dev/null || rm -f "$f"
  done <<< "$(guard_list_builds)"
}

guard_build_count() {
  guard_prune_stale_builds
  local n
  n="$(guard_list_builds | grep -c . || true)"
  printf '%d' "${n:-0}"
}

# Register this process as a build. Waits out a sweep that is already running,
# then registers and re-checks — the second look closes the window where a sweep
# claimed the slot between the first check and the registration.
# A sweep records its process in sweeping/pid. A sweep whose process is gone
# (killed, or its machine restarted) left the slot behind; remove it so builds
# do not wait on nobody. A slot with no pid file yet is a sweep starting up.
guard_drop_dead_sweep() {
  local g="$1" pid
  [[ -f "$g/sweeping/pid" ]] || return 0
  pid="$(cat "$g/sweeping/pid" 2>/dev/null)"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && return 0
  rm -f "$g/sweeping/pid" 2>/dev/null
  rmdir "$g/sweeping" 2>/dev/null || true
}

guard_build_enter() {
  local g; g="$(guard_dir)"
  mkdir -p "$g" 2>/dev/null || return 0
  local waited=0 limit="${GUARD_BUILD_WAIT_SECS:-300}"
  while :; do
    guard_drop_dead_sweep "$g"
    while [[ -d "$g/sweeping" ]] && (( waited < limit )); do
      guard_drop_dead_sweep "$g"
      [[ -d "$g/sweeping" ]] || break
      sleep 2; waited=$((waited + 2))
    done
    : > "$g/build.$$" 2>/dev/null || return 0
    if [[ -d "$g/sweeping" ]] && (( waited < limit )); then
      rm -f "$g/build.$$"
      continue
    fi
    return 0
  done
}

guard_build_leave() {
  rm -f "$(guard_dir)/build.$$" 2>/dev/null || true
}

# Claim the sweep slot. Returns non-zero when another sweep already holds it, or
# when builds are still running after the wait — the caller then does nothing,
# which is the safe outcome for a cleanup.
guard_sweep_enter() {
  local g; g="$(guard_dir)"
  mkdir -p "$g" 2>/dev/null || return 1
  guard_drop_dead_sweep "$g"
  mkdir "$g/sweeping" 2>/dev/null || return 1
  printf '%s\n' "${GUARD_OWNER_PID:-$$}" > "$g/sweeping/pid" 2>/dev/null || true
  local waited=0 limit="${GUARD_SWEEP_WAIT_SECS:-600}"
  while (( $(guard_build_count) > 0 )); do
    if (( waited >= limit )); then
      guard_sweep_leave
      return 1
    fi
    sleep 2; waited=$((waited + 2))
  done
  return 0
}

guard_sweep_leave() {
  rm -f "$(guard_dir)/sweeping/pid" 2>/dev/null
  rmdir "$(guard_dir)/sweeping" 2>/dev/null || true
}
