#!/usr/bin/env bash
# one-build.sh — `check -p mike --lib --tests` through scripts/cx's mbx opt-in
# into target/mbx-a, then the same into target/mbx-b, one after the other.
# Records for each: seconds, units restored from mbx's store, lowest available
# memory. Stops when available memory falls under MIN_AVAIL_MB (default 5000).
#
# Heavy: run it once, alone, never while another cargo job of yours runs.
# Results go to $OUT (default: a folder under TMPDIR); the last lines are a table.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${OUT:-${TMPDIR:-/tmp}/mbx-one-build}"
MIN_AVAIL_MB="${MIN_AVAIL_MB:-5000}"
mkdir -p "$OUT"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" CARGO_TERM_COLOR=never MBX_DISPLAY=plain MBX_SUMMARY=full
export MIKE_BUILD_CACHE=mbx SQLX_OFFLINE=true CARGO_INCREMENTAL=0
unset DATABASE_URL

avail_mb() { awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo; }

sampler() {
  local f="$1"
  while :; do
    local a; a="$(avail_mb)"
    echo "$a" >> "$f"
    if (( a < MIN_AVAIL_MB )); then touch "$OUT/STOP-LOW-MEMORY"; fi
    sleep 3
  done
}

run() {
  local name="$1"
  [[ -e "$OUT/STOP-LOW-MEMORY" ]] && { echo "$name skipped: available memory fell under ${MIN_AVAIL_MB} MiB"; return 1; }
  (( $(avail_mb) >= MIN_AVAIL_MB )) || { echo "$name not started: available memory under ${MIN_AVAIL_MB} MiB"; return 1; }
  : > "$OUT/$name.mem"
  sampler "$OUT/$name.mem" & local sp=$!
  local t0; t0=$(date +%s)
  CARGO_TARGET_DIR="target/$name" MBX_STATS_REPORT="$OUT/$name.json" \
    nice -n 19 ionice -c 3 "$root/scripts/cx" check -p mike --lib --tests --locked \
    > "$OUT/$name.log" 2>&1 &
  local bp=$!
  while kill -0 "$bp" 2>/dev/null; do
    if [[ -e "$OUT/STOP-LOW-MEMORY" ]]; then
      echo "$name stopped: available memory fell under ${MIN_AVAIL_MB} MiB"
      kill "$bp" 2>/dev/null
      break
    fi
    sleep 2
  done
  wait "$bp" 2>/dev/null; local rc=$?
  kill "$sp" 2>/dev/null; wait "$sp" 2>/dev/null
  local secs=$(( $(date +%s) - t0 ))
  local low; low="$(sort -n "$OUT/$name.mem" | head -1)"
  local hits; hits="$(grep -E 'mbx\[cache\]: [0-9]+ hits' "$OUT/$name.log" | tail -1)"
  printf '%s exit=%s seconds=%s lowest_available_mib=%s | %s\n' "$name" "$rc" "$secs" "${low:-?}" "${hits:-no mbx cache line}" | tee -a "$OUT/summary.txt"
  return "$rc"
}

: > "$OUT/summary.txt"
cd "$root" || exit 1
run mbx-a || exit 1
run mbx-b || exit 1
echo "done: $OUT/summary.txt"
