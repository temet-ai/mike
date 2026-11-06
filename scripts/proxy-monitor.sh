#!/usr/bin/env bash
# proxy-monitor.sh — real-time CPU/RSS monitor for mike with kill guard.
#
# Usage:
#   ./scripts/proxy-monitor.sh              # monitor only
#   KILL_GiB=3.5 ./scripts/proxy-monitor.sh  # kill at 3.5 GiB (default)
#   KILL_GiB=0   ./scripts/proxy-monitor.sh  # disable kill guard
#
# Columns: TIME | PID | RSS(ps) | FOOTPRINT | DELTA | RSS% | CPU% | UPTIME
#
#   RSS(ps)     — kernel RSS via `ps`, includes shared library pages.
#                 This is the metric that drives OOM pressure and the kill guard.
#   FOOTPRINT   — physical footprint via `top`, matches Activity Monitor "Memory".
#                 Excludes clean file-backed pages (shared libs not yet dirtied).
#
# Color: green < 50% ceiling | yellow 50–85% | red > 85% (high watermark)

set -euo pipefail

# ── config ────────────────────────────────────────────────────────────────────
KILL_GiB=${KILL_GiB:-3.5}
INTERVAL=${INTERVAL:-2}
RSS_CEILING_GiB=4
RSS_HIGH_PCT=85   # matches cap_manager config

# ── derived thresholds (all in KB) ───────────────────────────────────────────
CEILING_KB=$(awk "BEGIN{printf \"%d\", $RSS_CEILING_GiB * 1048576}")
HIGH_KB=$(awk    "BEGIN{printf \"%d\", $CEILING_KB * $RSS_HIGH_PCT / 100}")
WARN_KB=$(awk    "BEGIN{printf \"%d\", $CEILING_KB * 0.50}")
if [ "$(awk "BEGIN{print ($KILL_GiB > 0)}")" = "1" ]; then
  KILL_KB=$(awk "BEGIN{printf \"%d\", $KILL_GiB * 1048576}")
else
  KILL_KB=0
fi

# ── colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[0;33m'; GREEN='\033[0;32m'
BOLD='\033[1m';   DIM='\033[2m'; RESET='\033[0m'

# ── helpers ──────────────────────────────────────────────────────────────────
find_pid() {
  pgrep -x mike 2>/dev/null | head -1 \
    || ps aux 2>/dev/null \
         | awk '/[c]laude.proxy/ && !/cargo|rustc|clippy|pgrep|awk/' \
         | awk '{print $2}' | head -1
}

human_mb() {
  # $1 = KB; renders as MB or GiB
  awk "BEGIN{
    kb=$1
    mb = kb/1024
    if (mb >= 1024) { printf \"%.2f GiB\", mb/1024 }
    else            { printf \"%.1f MB\",  mb       }
  }"
}

# Parse the raw string `top` returns for MEM (e.g. "11M", "1.2G", "900K")
# and convert to KB for threshold comparisons.
top_mem_to_kb() {
  echo "$1" | awk '{
    v = $0
    unit = substr(v, length(v))
    num  = substr(v, 1, length(v)-1) + 0
    if      (unit == "G") { printf "%d", num * 1048576 }
    else if (unit == "M") { printf "%d", num * 1024    }
    else if (unit == "K") { printf "%d", num           }
    else                  { printf "%d", num / 1024    }  # bytes fallback
  }'
}

delta_str() {
  local kb=$1 prev=$2
  [ "$prev" -eq 0 ] && { echo "--"; return; }
  local d=$(( kb - prev ))
  local sign="+"
  [ "$d" -lt 0 ] && sign=""
  awk "BEGIN{printf \"%s%.1f MB\", \"$sign\", $d/1024}"
}

color_for_rss() {
  local kb=$1
  [ "$kb" -gt "$HIGH_KB" ] && { echo -n "$RED";    return; }
  [ "$kb" -gt "$WARN_KB" ] && { echo -n "$YELLOW"; return; }
  echo -n "$GREEN"
}

print_header() {
  printf "${BOLD}%-10s %-7s %-13s %-13s %-12s %-8s %-7s %-12s${RESET}\n" \
    TIME PID "RSS(ps)" "FOOTPRINT" DELTA RSS% CPU% UPTIME
  printf "${DIM}%-10s %-7s %-13s %-13s %-12s %-8s %-7s %-12s${RESET}\n" \
    "" "" "kernel/OOM" "activitymon" "" "" "" ""
  printf '%s\n' "$(printf '%.0s─' {1..85})"
}

# ── startup banner ────────────────────────────────────────────────────────────
clear
echo -e "${BOLD}mike real-time monitor${RESET}"
echo "  RSS ceiling : ${RSS_CEILING_GiB} GiB  |  high watermark : ${RSS_HIGH_PCT}% ($(human_mb "$HIGH_KB"))"
if [ "$KILL_KB" -gt 0 ]; then
  echo -e "  Kill guard  : ${RED}${KILL_GiB} GiB RSS(ps)${RESET} — will send SIGTERM and exit"
else
  echo "  Kill guard  : disabled (KILL_GiB=0)"
fi
echo "  Interval    : ${INTERVAL}s"
echo "  Memory log  : /tmp/mike-memory-logs/memory.log.\$(date +%%Y-%%m-%%d-%%H)"
echo ""
print_header

# ── main loop ────────────────────────────────────────────────────────────────
prev_rss_kb=0
last_pid=""

while true; do
  pid=$(find_pid)

  if [ -z "$pid" ]; then
    printf "%-10s  [proxy not running — waiting]\n" "$(date +%H:%M:%S)"
    prev_rss_kb=0
    last_pid=""
    sleep "$INTERVAL"
    continue
  fi

  # Reprint header when PID changes (restart detected)
  if [ "$pid" != "$last_pid" ] && [ -n "$last_pid" ]; then
    echo ""
    echo -e "${YELLOW}  proxy restarted — new PID $pid${RESET}"
    print_header
    prev_rss_kb=0
  fi
  last_pid="$pid"

  # ps: rss in KB, %cpu, %mem, etime
  read -r rss_kb cpu_pct etime 2>/dev/null \
    < <(ps -p "$pid" -o rss=,"%cpu"=,etime= 2>/dev/null \
          | awk '{gsub(/^[ \t]+/,""); print}') || { sleep "$INTERVAL"; continue; }

  [ -z "$rss_kb" ] && { sleep "$INTERVAL"; continue; }

  # footprint: top -l 1 matches Activity Monitor "Memory" column
  footprint_raw=$(top -pid "$pid" -l 1 -stats mem 2>/dev/null | tail -1)
  if [ "$footprint_raw" = "MEM" ] || [ -z "$footprint_raw" ]; then
    footprint_human="--"
  else
    footprint_kb=$(top_mem_to_kb "$footprint_raw")
    footprint_human=$(human_mb "$footprint_kb")
  fi

  ts=$(date +%H:%M:%S)
  rss_human=$(human_mb "$rss_kb")
  rss_pct=$(awk "BEGIN{printf \"%.1f%%\", $rss_kb * 100 / $CEILING_KB}")
  delta=$(delta_str "$rss_kb" "$prev_rss_kb")
  col=$(color_for_rss "$rss_kb")

  printf "${col}%-10s %-7s %-13s %-13s %-12s %-8s %-7s %-12s${RESET}\n" \
    "$ts" "$pid" "$rss_human" "$footprint_human" "$delta" "$rss_pct" "${cpu_pct}%" "$etime"

  # Kill guard (keyed on RSS — the OOM-pressure metric)
  if [ "$KILL_KB" -gt 0 ] && [ "$rss_kb" -gt "$KILL_KB" ]; then
    echo ""
    echo -e "${RED}${BOLD}  !! RSS(ps) $(human_mb "$rss_kb") > kill threshold $(human_mb "$KILL_KB")${RESET}"
    echo -e "${RED}${BOLD}  !! sending SIGTERM to PID $pid at $(date -u +%Y-%m-%dT%H:%M:%SZ)${RESET}"
    kill "$pid" 2>/dev/null && echo "  !! SIGTERM sent — proxy stopping" \
                            || echo "  !! kill failed (already gone?)"
    echo ""
    echo "  Last memory log lines:"
    tail -5 /tmp/mike-memory-logs/memory.log."$(date +%Y-%m-%d-%H)" 2>/dev/null \
      | awk '{printf "    %s\n", $0}' || echo "  (no log)"
    echo ""
    echo "  Monitor exiting."
    exit 0
  fi

  prev_rss_kb="$rss_kb"
  sleep "$INTERVAL"
done
