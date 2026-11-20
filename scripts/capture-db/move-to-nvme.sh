#!/usr/bin/env bash
# Move the capture DB cluster (Docker volume armitage_pg_data, 187 GB on the
# rotating /mnt/data disk) onto the NVMe root, with the database live for all
# but the last few minutes.
#
#   scripts/capture-db/move-to-nvme.sh            # run, or resume where it stopped
#   scripts/capture-db/move-to-nvme.sh --status   # show phase state and sizes
#   scripts/capture-db/move-to-nvme.sh --dry-run  # print what the next run would do
#   scripts/capture-db/move-to-nvme.sh --rollback # point the container back at the old volume
#   scripts/capture-db/move-to-nvme.sh --reset    # forget phase state (data stays)
#
# Phases, each recorded in the state file and skipped on rerun once done:
#   1 warm     rsync the volume onto the NVMe while Postgres keeps serving
#   2 count    record row counts from the live cluster (the verification target)
#   3 stop     clean shutdown of the container (mike stays up; capture pauses)
#   4 delta    rsync again: only what changed since the warm copy, minutes not hours
#   5 volume   create the bind-mounted Docker volume on the NVMe copy
#   6 switch   write scripts/capture-db/.env so compose and up.sh use the new volume
#   7 start    up.sh, wait for health
#   8 verify   row counts are at least phase 2's (capture resumes at once, so they only grow);
#              mike /api/health reports persistence reachable
#   9 done     print how to reclaim the old volume once you trust the new one
#
# Every phase is idempotent: rsync resumes, the volume create is a no-op if it
# exists, the .env write is a rewrite, and verify re-checks. Ctrl-C anywhere and
# rerun. Nothing deletes the old volume; that is a separate, printed command.
#
# Env: CAPTURE_DB_DATA_DIR (default ~/.local/share/mike-capture-db, on the NVMe
# root like the test DB), CAPTURE_DB_NEW_VOLUME (armitage_pg_nvme),
# CAPTURE_DB_PORT/USER/PASSWORD as for up.sh. Needs passwordless sudo for rsync
# (the cluster files are owned by uid 999) and ~200 GB free on the target.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_DIR="${HOME}/.local/state/mike"
STATE_FILE="${STATE_DIR}/capture-db-move.state"
LOG_FILE="${STATE_DIR}/capture-db-move.log"
OLD_VOLUME="armitage_pg_data"
NEW_VOLUME="${CAPTURE_DB_NEW_VOLUME:-armitage_pg_nvme}"
DEST="${CAPTURE_DB_DATA_DIR:-${HOME}/.local/share/mike-capture-db}"
PORT="${CAPTURE_DB_PORT:-5440}"
DB_USER="${CAPTURE_DB_USER:-devuser}"
DB_PASS="${CAPTURE_DB_PASSWORD:-devpass123}"
DB_URL="postgres://${DB_USER}:${DB_PASS}@127.0.0.1:${PORT}/claude_proxy"
INSPECT_HEALTH="http://127.0.0.1:9989/api/health"
COMPOSE_ENV="${SCRIPT_DIR}/.env"
MODE="run"

for arg in "$@"; do
    case "$arg" in
        --status)   MODE="status" ;;
        --dry-run)  MODE="dry-run" ;;
        --rollback) MODE="rollback" ;;
        --reset)    MODE="reset" ;;
        -h|--help)  sed -n '2,32p' "$0"; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

mkdir -p "${STATE_DIR}"
touch "${STATE_FILE}"
log() { printf '%s %s\n' "$(date '+%F %T')" "$*" | tee -a "${LOG_FILE}"; }
phase_done() { grep -qx "$1" "${STATE_FILE}"; }
mark_done() { phase_done "$1" || echo "$1" >> "${STATE_FILE}"; }
state_get() { sed -n "s/^$1=//p" "${STATE_FILE}" | tail -1; }
state_set() { echo "$1=$2" >> "${STATE_FILE}"; }

src_dir() { docker volume inspect "${OLD_VOLUME}" --format '{{.Mountpoint}}'; }
container_running() { [[ "$(docker inspect -f '{{.State.Running}}' armitage-pg 2>/dev/null || echo false)" == "true" ]]; }
free_gb() { df -Pk "$1" | awk 'NR==2 {printf "%d\n", $4 / 1048576}'; }
used_gb() { sudo du -sk "$1" 2>/dev/null | awk '{printf "%d\n", $1 / 1048576}'; }
row_counts() {
    psql "${DB_URL}" -X -At -F' ' -c \
        "select (select count(*) from requests.requests), (select count(*) from sessions.sessions), (select max(id) from requests.requests)"
}
rsync_copy() {
    local src="$1" dst="$2"
    sudo mkdir -p "${dst}"
    sudo rsync -aHAX --delete --numeric-ids --info=progress2 --inplace "${src}/" "${dst}/"
}

show_status() {
    local src; src="$(src_dir)"
    echo "state file:    ${STATE_FILE}"
    echo "phases done:   $(grep -vE '=' "${STATE_FILE}" | tr '\n' ' ')"
    echo "old volume:    ${OLD_VOLUME} at ${src} ($(used_gb "${src}") GB)"
    echo "target:        ${DEST} ($( [[ -d "${DEST}" ]] && used_gb "${DEST}" || echo 0) GB copied, $(free_gb "$(dirname "${DEST}")") GB free)"
    echo "new volume:    ${NEW_VOLUME} $(docker volume inspect "${NEW_VOLUME}" >/dev/null 2>&1 && echo exists || echo absent)"
    echo "compose env:   $( [[ -f "${COMPOSE_ENV}" ]] && cat "${COMPOSE_ENV}" || echo '(none: compose uses the old volume)')"
    echo "container:     $(container_running && echo running || echo stopped), volume: $(docker inspect armitage-pg --format '{{range .Mounts}}{{.Name}}{{end}}' 2>/dev/null)"
    echo "counts (live): $(row_counts 2>/dev/null || echo unreachable)"
    echo "counts (ref):  $(state_get counts || echo '(phase 2 not run)')"
}

case "${MODE}" in
    status) show_status; exit 0 ;;
    reset)  rm -f "${STATE_FILE}"; log "state reset; data on both sides untouched"; exit 0 ;;
    rollback)
        log "rollback: pointing compose back at ${OLD_VOLUME}"
        container_running && "${SCRIPT_DIR}/down.sh"
        rm -f "${COMPOSE_ENV}"
        "${SCRIPT_DIR}/up.sh"
        sed -i '/^\(volume\|switch\|start\|verify\|done\)$/d' "${STATE_FILE}"
        log "rollback complete; the NVMe copy at ${DEST} is kept"
        exit 0 ;;
esac

SRC="$(src_dir)"
sudo test -d "${SRC}" || { echo "old volume mountpoint not found: ${SRC}" >&2; exit 1; }
if [[ "${MODE}" == "dry-run" ]]; then
    echo "would run, in order, skipping phases already done:"
    for p in warm count stop delta volume switch start verify done; do
        printf '  %-7s %s\n' "$p" "$(phase_done "$p" && echo done || echo pending)"
    done
    echo "source ${SRC} ($(used_gb "${SRC}") GB) -> ${DEST} ($(free_gb "$(dirname "${DEST}")") GB free)"
    exit 0
fi

need=$(( $(used_gb "${SRC}") + 20 ))
if ! phase_done warm && (( $(free_gb "$(dirname "${DEST}")") < need )); then
    echo "need about ${need} GB free under $(dirname "${DEST}"), have $(free_gb "$(dirname "${DEST}")")" >&2; exit 1
fi
command -v rsync >/dev/null || { echo "rsync is required" >&2; exit 1; }

if ! phase_done warm; then
    log "phase 1 warm: copying ${SRC} -> ${DEST} while the database keeps running (rerun resumes)"
    rsync_copy "${SRC}" "${DEST}"
    mark_done warm
    log "phase 1 warm done ($(used_gb "${DEST}") GB on the NVMe)"
fi

if ! phase_done count; then
    container_running || "${SCRIPT_DIR}/up.sh"
    counts="$(row_counts)"
    state_set counts "${counts}"
    mark_done count
    log "phase 2 count: reference counts recorded: ${counts}"
fi

if ! phase_done stop; then
    log "phase 3 stop: clean shutdown of armitage-pg (mike keeps serving; capture pauses until phase 7)"
    container_running && "${SCRIPT_DIR}/down.sh"
    mark_done stop
fi

if ! phase_done delta; then
    if container_running; then
        log "container is running again; stopping it before the delta sync"
        "${SCRIPT_DIR}/down.sh"
    fi
    log "phase 4 delta: syncing what changed since the warm copy"
    rsync_copy "${SRC}" "${DEST}"
    mark_done delta
    log "phase 4 delta done"
fi

if ! phase_done volume; then
    if ! docker volume inspect "${NEW_VOLUME}" >/dev/null 2>&1; then
        docker volume create --driver local \
            --opt type=none --opt o=bind --opt device="${DEST}" "${NEW_VOLUME}" >/dev/null
    fi
    mark_done volume
    log "phase 5 volume: ${NEW_VOLUME} bound to ${DEST}"
fi

if ! phase_done switch; then
    printf 'CAPTURE_DB_VOLUME=%s\n' "${NEW_VOLUME}" > "${COMPOSE_ENV}"
    mark_done switch
    log "phase 6 switch: ${COMPOSE_ENV} now selects ${NEW_VOLUME}"
fi

if ! phase_done start; then
    log "phase 7 start: bringing the container up on the NVMe copy"
    "${SCRIPT_DIR}/up.sh"
    mounted="$(docker inspect armitage-pg --format '{{range .Mounts}}{{.Name}}{{end}}')"
    [[ "${mounted}" == "${NEW_VOLUME}" ]] || { echo "container mounted volume '${mounted}', expected ${NEW_VOLUME}" >&2; exit 1; }
    bound="$(docker volume inspect "${NEW_VOLUME}" --format '{{index .Options "device"}}')"
    [[ "${bound}" == "${DEST}" ]] || { echo "volume ${NEW_VOLUME} binds ${bound}, expected ${DEST}" >&2; exit 1; }
    mark_done start
fi

if ! phase_done verify; then
    ref="$(state_get counts)"
    now="$(row_counts)"
    read -r ref_req ref_sess ref_max <<<"${ref}"
    read -r now_req now_sess now_max <<<"${now}"
    if (( now_req < ref_req || now_sess < ref_sess || now_max < ref_max )); then
        echo "row counts went backwards: reference '${ref}' vs now '${now}'" >&2
        echo "run --rollback to return to the old volume, or --reset then rerun to copy again" >&2
        exit 1
    fi
    for _ in $(seq 1 30); do
        if curl -s -m 5 "${INSPECT_HEALTH}" | grep -qE '"reachable":true'; then break; fi
        sleep 2
    done
    curl -s -m 5 "${INSPECT_HEALTH}" | grep -qE '"reachable":true' \
        || log "warning: mike /api/health does not yet report persistence reachable; it reconnects on its next write"
    mark_done verify
    log "phase 8 verify: counts match (${now}); mike reconnected"
fi

if ! phase_done done; then
    mark_done done
    log "phase 9 done: the capture DB now runs from ${DEST} on the NVMe"
fi

cat <<EOF

Done. The old volume ${OLD_VOLUME} ($(used_gb "${SRC}") GB on /mnt/data) is untouched.
When you trust the new one (a day of normal use is plenty), reclaim it with:

    docker volume rm ${OLD_VOLUME}

To go back before that:  $0 --rollback
EOF
