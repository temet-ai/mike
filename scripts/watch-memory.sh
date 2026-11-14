#!/usr/bin/env bash
# watch-memory.sh — tail the proxy memory ticker in real time.
#
# Usage:
#   ./scripts/watch-memory.sh          # today's log, formatted
#   ./scripts/watch-memory.sh --raw    # today's log, raw JSON lines

set -euo pipefail

LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/mike/memory-logs"
TODAY="$(date +%Y-%m-%d-%H)"
LOG_FILE="${LOG_DIR}/memory.log.${TODAY}"

if [[ ! -f "${LOG_FILE}" ]]; then
    LOG_FILE="$(ls -t "${LOG_DIR}"/memory.log.* 2>/dev/null | head -1 || true)"
    if [[ -z "${LOG_FILE}" ]]; then
        echo "No memory log files found in ${LOG_DIR}" >&2
        exit 1
    fi
fi

echo "==> Watching ${LOG_FILE}" >&2

if [[ "${1:-}" == "--raw" ]]; then
    exec tail -f "${LOG_FILE}" | grep --line-buffered memory.tick
fi

exec tail -f "${LOG_FILE}" | grep --line-buffered memory.tick | python3 -u -c '
import sys, json
for line in sys.stdin:
    try:
        idx = line.index("memory.tick")
        d = json.loads(line[idx + 11:].strip())
        rss     = d.get("rss_peak_bytes", 0)
        fe      = d.get("fastembed_model_bytes", 0)
        entries = d.get("store_entries", 0)
        store   = d.get("store_bytes", 0)
        ts      = line[:23]
        parts   = [f"rss={rss/1024/1024:.1f} MiB"]
        if entries > 0:
            parts.append(f"entries={entries}")
        if store > 0:
            parts.append(f"store={store/1024:.0f} KB")
        if fe > 0:
            parts.append(f"fastembed={fe/1024/1024:.0f} MiB")
        print(f"{ts}  " + "  ".join(parts), flush=True)
    except Exception:
        pass
'
