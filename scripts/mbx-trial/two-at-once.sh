#!/usr/bin/env bash
# two-at-once.sh — the two builds of one-build.sh started together: `check -p
# mike --lib --tests` through scripts/cx's mbx opt-in into target/mbx-a and
# target/mbx-b at the same moment. Records both times and the lowest available
# memory. Stops both when a STOP file appears in $OUT or available memory falls
# under MIN_AVAIL_MB (default 5000).
#
# NOT run by any fleet seat. The lead session runs it once, when no seat is
# building. It decides whether two builds in one checkout, each in its own build
# directory, finish faster than the same two queued behind one cargo lock.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${OUT:-${TMPDIR:-/tmp}/mbx-two-at-once}"
MIN_AVAIL_MB="${MIN_AVAIL_MB:-5000}"
mkdir -p "$OUT"
rm -f "$OUT/STOP" "$OUT/STOP-LOW-MEMORY"

export CARGO_BUILD_JOBS=6 CARGO_TERM_COLOR=never MBX_DISPLAY=plain MBX_SUMMARY=full
export MIKE_BUILD_CACHE=mbx SQLX_OFFLINE=true
unset DATABASE_URL

avail_mb() { awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo; }

(( $(avail_mb) >= MIN_AVAIL_MB )) || { echo "not started: available memory under ${MIN_AVAIL_MB} MiB"; exit 1; }
cd "$root" || exit 1

: > "$OUT/all.mem"
t0=$(date +%s)
pids=()
for name in mbx-a mbx-b; do
  (
    s=$(date +%s)
    CARGO_TARGET_DIR="target/$name" MBX_STATS_REPORT="$OUT/$name.json" \
      nice -n 19 ionice -c 3 "$root/scripts/cx" check -p mike --lib --tests --locked \
      > "$OUT/$name.log" 2>&1
    rc=$?
    echo "$name exit=$rc seconds=$(( $(date +%s) - s ))" > "$OUT/$name.result"
  ) &
  pids+=($!)
done

stopped=""
while :; do
  alive=0
  for p in "${pids[@]}"; do kill -0 "$p" 2>/dev/null && alive=1; done
  (( alive )) || break
  a="$(avail_mb)"
  echo "$a" >> "$OUT/all.mem"
  if [[ -e "$OUT/STOP" ]]; then stopped="STOP file"; fi
  if (( a < MIN_AVAIL_MB )); then stopped="available memory ${a} MiB under ${MIN_AVAIL_MB}"; fi
  if [[ -n "$stopped" ]]; then
    echo "stopping both builds: $stopped"
    for p in "${pids[@]}"; do pkill -TERM -P "$p" 2>/dev/null; kill "$p" 2>/dev/null; done
    break
  fi
  sleep 3
done
wait 2>/dev/null

low="$(sort -n "$OUT/all.mem" | head -1)"
{
  cat "$OUT"/mbx-a.result "$OUT"/mbx-b.result 2>/dev/null
  echo "wall_seconds=$(( $(date +%s) - t0 )) lowest_available_mib=${low:-?} stopped=${stopped:-no}"
  for n in mbx-a mbx-b; do
    echo "$n: $(grep -E 'mbx\[cache\]: [0-9]+ hits' "$OUT/$n.log" 2>/dev/null | tail -1)"
  done
} | tee "$OUT/summary.txt"
