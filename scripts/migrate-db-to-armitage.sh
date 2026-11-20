#!/usr/bin/env bash
# migrate-db-to-armitage.sh — move the capture DB from infra (Pi) to armitage.
#
# STATUS (2026-09-14): the migration is done and the WSL2 era is over. The
#   capture DB now runs in Docker directly on armitage's Arch Linux host
#   (scripts/capture-db/). `lan-check`, `lan-fix` and `tunnel` target the old
#   Windows portproxy/WSL2 topology and no longer apply; kept for the record.
#
# WHY THIS SHAPE
#   The Pi is ~95% full, so nothing may be written there: pg_dump runs as a CLIENT
#   on armitage, pulling over TCP. The Pi only reads.
#
#   A single 1-4h stream that restarts from zero on any blip is a bad bet (an SSH
#   drop to armitage was observed while planning this). So the dump is split into
#   independently retryable UNITS — pre-data, one per table, and the giant
#   requests.requests chunked by id range — each recorded in a manifest with its
#   row count. Re-running skips completed units. Nothing is ever half-applied.
#
#   The restore lands in a STAGING database and is swapped in by atomic rename
#   only after verification passes. The Pi is never modified, so rollback is
#   reverting three config values.
#
# USAGE
#   scripts/migrate-db-to-armitage.sh <command>
#
#     preflight   Check both ends. Safe, read-only, changes nothing.
#     dump        Pull every unit to armitage. Resumable — re-run after a failure.
#     restore     Load the units into the staging DB (local to armitage).
#     verify      Compare staging against the source snapshot. Gates the swap.
#     swap        Atomic rename staging -> live. Refuses unless verify passed.
#     cutover     Repoint this repo's config at armitage's LAN address.
#     rollback    Repoint config back at the Pi.
#     lan-check   Is armitage:5440 reachable from here, and if not, why not.
#     lan-fix     Install/repair the Windows portproxy rule that exposes 5440.
#     tunnel      Fallback SSH tunnel, for when the LAN path is unavailable.
#     status      Where the run is up to. Safe to call any time, including mid-run.
#     run         preflight -> dump -> restore -> verify -> swap (stops before cutover).
#
#   The DB link runs over the LAN to armitage:5440 (see ARM_IP below), carried by a
#   Windows portproxy rule. `lan-fix` installs or repairs it, unattended. Re-run it
#   after a WSL restart: the rule pins a WSL IP that moves. `lan-check` tells you.
#
# PRECONDITION: mike is stopped (scripts/stop-proxy.sh). `preflight` enforces it —
# a dump taken while capture is writing is not a consistent snapshot.
set -uo pipefail

# ── config ────────────────────────────────────────────────────────────────────
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SRC_HOST="${SRC_HOST:-192.168.68.136}"      # Pi by IP: .local mDNS does not resolve inside containers
SRC_PORT="${SRC_PORT:-5440}"
SRC_USER="${SRC_USER:-devuser}"
SRC_DB="${SRC_DB:-claude_proxy}"

ARM_SSH="${ARM_SSH:-armitage}"
# The Mac dials armitage's LAN address directly.
#
# WSL2 is in NAT mode, so a LAN packet for the Windows host does not reach Ubuntu
# by itself. An elevated `netsh portproxy` rule on Windows bridges it — the same
# mechanism already carrying neo4j, ollama and the rest on that box. `lan-fix`
# installs and repairs it; `lan-check` reports whether it is currently working.
#
# The rule pins the WSL IP, which moves whenever the distro restarts, so a WSL
# restart needs one `lan-fix` (it re-detects the IP). `tunnel` stays available as
# a fallback for when the LAN path is not an option — off-network, or a portproxy
# rule that cannot be repaired: dial it with ARM_IP=127.0.0.1 ARM_PORT=15440.
ARM_IP="${ARM_IP:-192.168.68.122}"
TUNNEL_PORT="${TUNNEL_PORT:-15440}"
ARM_CONTAINER="${ARM_CONTAINER:-armitage-pg}"
ARM_PORT="${ARM_PORT:-5440}"               # what THIS Mac dials, on armitage's LAN address
ARM_PORT_REMOTE="${ARM_PORT_REMOTE:-5440}" # what the container publishes inside WSL
ARM_USER="${ARM_USER:-devuser}"
PG_IMAGE="${PG_IMAGE:-postgres:19rc1-bookworm@sha256:d3d249b3dcc7920ab06f437a49ceb8fb83d483d7863bef31e1bf1d4bdc584cc2}"   # PostgreSQL 19 client tools

# Dump lands on E: by default — big, and it keeps the WSL .vhdx from growing ~80GB
# permanently (it never shrinks without an explicit compact). Set DUMP_DIR to a
# path on the ext4 disk instead if you would rather trade that for speed.
DUMP_DIR="${DUMP_DIR:-/mnt/e/db-migrate/dumps/claude_proxy}"
STAGING="${STAGING:-${SRC_DB}__migrate_staging}"

# requests.requests averages ~1MB/row (bodies live in TOAST), so keep chunks modest.
CHUNK_ROWS="${CHUNK_ROWS:-2000}"
BIG_TABLE="${BIG_TABLE:-requests.requests}"
RESTORE_JOBS="${RESTORE_JOBS:-8}"

# Per-database, so a rehearsal (SRC_DB=devdb) can never leave counts behind that a
# later claude_proxy run would silently verify against.
STATE_LOCAL="${PROJECT_ROOT}/artifacts/2026-08-18/db-migration-armitage/state/${SRC_DB}"
MANIFEST="${DUMP_DIR}/manifest.tsv"          # on armitage
SRC_COUNTS="${STATE_LOCAL}/source-counts.tsv"
VERIFY_OK="${STATE_LOCAL}/verify.ok"

SSH_OPTS=(-o ConnectTimeout=15 -o ServerAliveInterval=15 -o ServerAliveCountMax=8)

# ── output ────────────────────────────────────────────────────────────────────
c_i=$'\033[1;34m'; c_ok=$'\033[1;32m'; c_w=$'\033[1;33m'; c_e=$'\033[1;31m'; c_0=$'\033[0m'
log()  { printf '%s[migrate]%s %s\n' "$c_i" "$c_0" "$*"; }
ok()   { printf '%s   ✓%s %s\n' "$c_ok" "$c_0" "$*"; }
warn() { printf '%s   !%s %s\n' "$c_w" "$c_0" "$*" >&2; }
die()  { printf '%s[error]%s %s\n' "$c_e" "$c_0" "$*" >&2; exit 1; }

# ── credentials ───────────────────────────────────────────────────────────────
# Read from .env rather than hardcoding, and never echo them.
src_pw() { sed -nE 's#^MIKE_DATABASE_URL="?postgres://[^:]+:([^@]+)@.*#\1#p' "${PROJECT_ROOT}/.env" | head -1; }
arm_pw() { ssh "${SSH_OPTS[@]}" "$ARM_SSH" 'sed -nE "s/^TARGET_PASSWORD=(.*)/\1/p" /mnt/e/projects/db-migrate/.env | head -1' 2>/dev/null; }
SRC_PW=""; ARM_PW=""

# ── remote helpers ────────────────────────────────────────────────────────────
# Remote commands are sent over stdin (bash -s) so we never fight nested quoting.
arm() { ssh "${SSH_OPTS[@]}" "$ARM_SSH" bash -s; }

# psql against the TARGET, over its local socket inside the container (trust auth).
arm_q() { # arm_q <db> <sql>
  printf 'docker exec %s psql -U %s -d %q -tAqc %q\n' "$ARM_CONTAINER" "$ARM_USER" "$1" "$2" | arm
}
# pg_dump/pg_restore/psql run in a THROWAWAY client container, not in the running
# server container. Two reasons: the dump directory is a host path the server
# container cannot see (it mounts only pgdata), and `pg_restore -j` needs a real
# file rather than a stream. --network host lets one container reach both the Pi
# and the local target. Nothing about the running server is modified.
pgtool() { # pgtool <password> <cmd> [args...]
  local pw="$1"; shift
  local cmd a
  cmd="$(printf 'docker run --rm --network host -e PGPASSWORD=%q -v %q:/dumps %s' \
          "$pw" "$DUMP_DIR" "$PG_IMAGE")"
  for a in "$@"; do cmd+=" $(printf '%q' "$a")"; done
  printf '%s\n' "$cmd" | arm
}

# Multi-line SQL to the TARGET, over stdin — avoids nesting quotes through
# ssh -> docker exec -> psql, which is where this kind of script usually breaks.
arm_sql() { # arm_sql <db>   (SQL on stdin)
  { printf 'docker exec -i %s psql -U %s -d %q -v ON_ERROR_STOP=1 -q <<'"'"'__SQL_EOF__'"'"'\n' \
      "$ARM_CONTAINER" "$ARM_USER" "$1"
    cat
    printf '\n__SQL_EOF__\n'
  } | arm
}
# psql against the SOURCE, from this Mac (the Pi's 5440 is reachable here).
src_q() { # src_q <db> <sql>
  PGPASSWORD="$SRC_PW" psql -h "$SRC_HOST" -p "$SRC_PORT" -U "$SRC_USER" -d "$1" -tAqc "$2" 2>&1
}

manifest_get() { # manifest_get <unit> -> "status<TAB>rows"
  printf 'grep -P %q %q 2>/dev/null | tail -1 | cut -f2,3\n' "^$(printf '%s' "$1" | sed 's/[][\.*^$]/\\&/g')\t" "$MANIFEST" | arm
}
manifest_put() { # manifest_put <unit> <status> <rows> <bytes> <secs>
  printf 'mkdir -p %q && printf "%%s\\t%%s\\t%%s\\t%%s\\t%%s\\n" %q %q %q %q %q >> %q\n' \
    "$DUMP_DIR" "$1" "$2" "$3" "$4" "$5" "$MANIFEST" | arm
}

# ── unit list ─────────────────────────────────────────────────────────────────
# Built from the source catalogue, so a schema change cannot silently drop a table.
list_tables() {
  src_q "$SRC_DB" "
    SELECT n.nspname||'.'||c.relname
    FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
    WHERE c.relkind='r' AND n.nspname NOT IN ('pg_catalog','information_schema','pg_toast')
    ORDER BY 1;"
}

# ── preflight ─────────────────────────────────────────────────────────────────
cmd_preflight() {
  log "preflight"
  local fail=0

  SRC_PW="$(src_pw)"; [[ -n "$SRC_PW" ]] || die "could not read the source password from .env"

  # 1. mike must be stopped — a dump taken mid-capture is not a consistent snapshot.
  #    Only enforced for the capture DB itself, so a rehearsal on a small database
  #    (SRC_DB=devdb) can prove the pipeline without an outage.
  if [[ "$SRC_DB" == "claude_proxy" ]]; then
    if lsof -ti :9988 >/dev/null 2>&1 || lsof -ti :9989 >/dev/null 2>&1; then
      warn "mike is RUNNING (:9988/:9989). Stop it first: scripts/stop-proxy.sh"; fail=1
    else ok "mike is stopped"; fi
  else
    ok "rehearsal on '${SRC_DB}' — mike check skipped (not the capture DB)"
  fi

  # 2. Source reachable, and quiet.
  local v; v="$(src_q postgres 'SHOW server_version')"
  [[ "$v" == 17.* ]] || { warn "source version unexpected: $v"; fail=1; }
  ok "source ${SRC_HOST}:${SRC_PORT} is PostgreSQL $v"
  local busy; busy="$(src_q "$SRC_DB" "SELECT count(*) FROM pg_stat_activity WHERE datname='${SRC_DB}' AND state<>'idle' AND pid<>pg_backend_pid()")"
  [[ "${busy:-0}" == 0 ]] && ok "no active sessions on the source" || warn "source has ${busy} active session(s) — capture may still be writing"

  # 3. Target container healthy.
  ARM_PW="$(arm_pw)"
  local tv; tv="$(arm_q postgres 'SHOW server_version')"
  [[ "$tv" == 17.* ]] || { warn "target version unexpected: $tv"; fail=1; }
  ok "target ${ARM_SSH}/${ARM_CONTAINER} is PostgreSQL $tv"

  # 4. pg_restore must not be OLDER than the dump's producer. Same-major is required;
  #    restoring a NEWER dump into an OLDER server is the unsupported direction.
  #    Compared as integers (server_version_num) — string sorting a version that
  #    carries a "(Debian ...)" suffix only works by luck.
  local sn tn; sn="$(src_q postgres 'SHOW server_version_num')"; tn="$(arm_q postgres 'SHOW server_version_num')"
  [[ $(( sn / 10000 )) == $(( tn / 10000 )) ]] \
    || { warn "major version mismatch: source $(( sn / 10000 )) vs target $(( tn / 10000 ))"; fail=1; }
  if (( tn < sn )); then
    warn "target ($tn) is OLDER than source ($sn) — restore may reject the dump"; fail=1
  else ok "version direction is safe (${sn} -> ${tn})"; fi

  # 5. Every source extension must exist on the target.
  local missing
  missing="$(src_q "$SRC_DB" "SELECT extname FROM pg_extension WHERE extname<>'plpgsql' ORDER BY 1" | while read -r e; do
      [[ -z "$e" ]] && continue
      local got; got="$(arm_q postgres "SELECT 1 FROM pg_available_extensions WHERE name='${e}'")"
      [[ "$got" == 1 ]] || printf '%s ' "$e"
    done)"
  [[ -z "$missing" ]] && ok "all source extensions available on target" || { warn "target is missing extensions: $missing"; fail=1; }

  # 6. Target must be able to reach the Pi — the dump is pulled, not pushed.
  local reach; reach="$(printf 'docker exec %s bash -c "timeout 8 bash -c \\"cat < /dev/null > /dev/tcp/%s/%s\\"" >/dev/null 2>&1 && echo yes || echo no\n' \
      "$ARM_CONTAINER" "$SRC_HOST" "$SRC_PORT" | arm)"
  [[ "$reach" == yes ]] && ok "target can reach the source over TCP" || { warn "target CANNOT reach ${SRC_HOST}:${SRC_PORT}"; fail=1; }

  # 7. Room for the dump.
  local avail; avail="$(printf 'mkdir -p %q 2>/dev/null; df -Pm %q | tail -1 | awk "{print \\$4}"\n' "$DUMP_DIR" "$DUMP_DIR" | arm)"
  ok "dump target ${DUMP_DIR} has ${avail}MB free"
  [[ "${avail:-0}" -gt 150000 ]] || { warn "less than ~150GB free for the dump"; fail=1; }

  # 8. Cutover-only: is 5440 open from here? Not required until `cutover`.
  if nc -z -G 5 "$ARM_IP" "$ARM_PORT" >/dev/null 2>&1; then
    ok "armitage:${ARM_PORT} reachable from this Mac (cutover unblocked)"
  else
    warn "armitage:${ARM_PORT} NOT reachable from this Mac — dump/restore can still proceed;"
    warn "  cutover is blocked until WSL networking is resolved (mirrored mode, or a portproxy rule)."
  fi

  (( fail == 0 )) && { log "preflight PASS"; return 0; } || die "preflight FAILED — fix the items above"
}

# ── snapshot source truth ─────────────────────────────────────────────────────
# Captured BEFORE the dump so verification grades against evidence, not assumption.
snapshot_source() {
  mkdir -p "$STATE_LOCAL"
  [[ -s "$SRC_COUNTS" ]] && { ok "source snapshot already recorded"; return 0; }
  log "recording source row counts (exact — this is the evidence verify grades against)"
  : > "$SRC_COUNTS"
  local t n
  while read -r t; do
    [[ -z "$t" ]] && continue
    n="$(src_q "$SRC_DB" "SELECT count(*) FROM ${t}")"
    printf '%s\t%s\n' "$t" "$n" >> "$SRC_COUNTS"
    printf '    %-45s %s\n' "$t" "$n"
  done < <(list_tables)
  ok "recorded $(wc -l < "$SRC_COUNTS" | tr -d ' ') tables"
}

# ── dump ──────────────────────────────────────────────────────────────────────
dump_unit_done() { [[ "$(manifest_get "$1" | cut -f1)" == done ]]; }

cmd_dump() {
  SRC_PW="${SRC_PW:-$(src_pw)}"
  snapshot_source
  log "dumping to ${ARM_SSH}:${DUMP_DIR} (uncompressed: LAN, and it keeps the Pi's CPU out of the path)"
  printf 'mkdir -p %q\n' "$DUMP_DIR" | arm

  # -- pre-data: schema WITHOUT indexes/constraints, so data loads fast --------
  if dump_unit_done "__pre"; then ok "pre-data (cached)"; else
    local t0=$SECONDS
    pgtool "$SRC_PW" pg_dump -h "$SRC_HOST" -p "$SRC_PORT" -U "$SRC_USER" -d "$SRC_DB" \
      --section=pre-data -Fc -Z 0 --no-owner --no-privileges -f /dumps/pre.dump \
      || die "pre-data dump failed"
    manifest_put "__pre" done 0 0 $((SECONDS-t0)); ok "pre-data"
  fi

  # -- per-table data, one retryable unit each --------------------------------
  local t rows n=0 total
  total="$(wc -l < "$SRC_COUNTS" | tr -d ' ')"
  while read -r t rows; do
    [[ -z "$t" ]] && continue
    n=$((n+1))
    [[ "$t" == "$BIG_TABLE" ]] && continue          # chunked separately below
    if dump_unit_done "$t"; then printf '    [%d/%d] %-45s cached\n' "$n" "$total" "$t"; continue; fi
    local t0=$SECONDS
    pgtool "$SRC_PW" pg_dump -h "$SRC_HOST" -p "$SRC_PORT" -U "$SRC_USER" -d "$SRC_DB" \
      --section=data -Fc -Z 0 --no-owner --no-privileges -t "$t" -f "/dumps/data.${t//./_}.dump" \
      || { warn "FAILED $t — re-run 'dump' to retry just this unit"; return 1; }
    manifest_put "$t" done "$rows" 0 $((SECONDS-t0))
    printf '    [%d/%d] %-45s %s rows  %ss\n' "$n" "$total" "$t" "$rows" "$((SECONDS-t0))"
  done < "$SRC_COUNTS"

  dump_big_table || return 1
  ok "dump complete"
}

# The one table too big to be a single retry unit. Chunk boundaries come from an
# exact row-number walk, so chunks are well-defined and reproducible across runs.
dump_big_table() {
  local rows; rows="$(awk -F'\t' -v t="$BIG_TABLE" '$1==t{print $2}' "$SRC_COUNTS")"
  [[ -z "$rows" || "$rows" == 0 ]] && { ok "${BIG_TABLE}: empty, nothing to chunk"; return 0; }
  log "${BIG_TABLE}: ${rows} rows -> chunks of ${CHUNK_ROWS}"

  local bounds; bounds="$(src_q "$SRC_DB" "
    SELECT id FROM (SELECT id, row_number() OVER (ORDER BY id) rn FROM ${BIG_TABLE}) s
    WHERE (rn-1) % ${CHUNK_ROWS} = 0 ORDER BY id")"
  local maxid; maxid="$(src_q "$SRC_DB" "SELECT max(id) FROM ${BIG_TABLE}")"

  local -a b=(); while read -r x; do [[ -n "$x" ]] && b+=("$x"); done <<< "$bounds"
  local i lo hi unit t0 got
  for i in "${!b[@]}"; do
    lo="${b[$i]}"
    if [[ $((i+1)) -lt ${#b[@]} ]]; then hi="${b[$((i+1))]}"; else hi=$((maxid+1)); fi
    unit="${BIG_TABLE}#${lo}"
    if dump_unit_done "$unit"; then printf '    chunk %-4d %-22s cached\n' "$i" "[${lo},${hi})"; continue; fi
    t0=$SECONDS
    # \copy writes client-side, i.e. into the mounted /dumps — exact binary format.
    pgtool "$SRC_PW" psql -h "$SRC_HOST" -p "$SRC_PORT" -U "$SRC_USER" -d "$SRC_DB" -v ON_ERROR_STOP=1 -q \
      -c "\\copy (SELECT * FROM ${BIG_TABLE} WHERE id >= ${lo} AND id < ${hi}) TO '/dumps/big.${lo}.bin' WITH (FORMAT binary)" \
      || { warn "FAILED chunk [${lo},${hi}) — re-run 'dump' to retry it alone"; return 1; }
    got="$(src_q "$SRC_DB" "SELECT count(*) FROM ${BIG_TABLE} WHERE id >= ${lo} AND id < ${hi}")"
    manifest_put "$unit" done "$got" 0 $((SECONDS-t0))
    printf '    chunk %-4d %-22s %s rows  %ss\n' "$i" "[${lo},${hi})" "$got" "$((SECONDS-t0))"
  done
  ok "${BIG_TABLE}: all chunks captured"
}

# ── restore ───────────────────────────────────────────────────────────────────
cmd_restore() {
  log "restoring into staging '${STAGING}' (local to armitage — no network in this path)"
  # Fresh staging every time: a partial restore is never reused.
  printf 'docker exec %s psql -U %s -d postgres -q -c "DROP DATABASE IF EXISTS %s WITH (FORCE);" -c "CREATE DATABASE %s;"\n' \
    "$ARM_CONTAINER" "$ARM_USER" "$STAGING" "$STAGING" | arm || die "could not create staging DB"
  ok "staging created"

  ARM_PW="${ARM_PW:-$(arm_pw)}"
  pgtool "$ARM_PW" pg_restore -h 127.0.0.1 -p "$ARM_PORT_REMOTE" -U "$ARM_USER" -d "$STAGING" \
    --exit-on-error --no-owner --no-privileges /dumps/pre.dump || die "pre-data restore failed"
  ok "schema (pre-data)"

  # Data, in any order — FKs do not exist yet, they arrive with post-data.
  local t
  while read -r t _; do
    [[ -z "$t" || "$t" == "$BIG_TABLE" ]] && continue
    local f="/dumps/data.${t//./_}.dump"
    printf '[ -s %q ] && echo yes || echo no\n' "${DUMP_DIR}/data.${t//./_}.dump" | arm | grep -q yes || continue
    pgtool "$ARM_PW" pg_restore -h 127.0.0.1 -p "$ARM_PORT_REMOTE" -U "$ARM_USER" -d "$STAGING" \
      --exit-on-error --no-owner --no-privileges --data-only "$f" || die "data restore failed for $t"
    printf '    %-45s loaded\n' "$t"
  done < "$SRC_COUNTS"

  # Big table chunks, in id order (ls -v so 10 sorts after 9, not after 1).
  local chunks; chunks="$(printf 'ls -1v %q/big.*.bin 2>/dev/null | xargs -n1 basename 2>/dev/null\n' "$DUMP_DIR" | arm)"
  local cf
  while read -r cf; do
    [[ -z "$cf" ]] && continue
    pgtool "$ARM_PW" psql -h 127.0.0.1 -p "$ARM_PORT_REMOTE" -U "$ARM_USER" -d "$STAGING" -v ON_ERROR_STOP=1 -q \
      -c "\\copy ${BIG_TABLE} FROM '/dumps/${cf}' WITH (FORMAT binary)" \
      || die "chunk restore failed: ${cf}"
    printf '    %-45s loaded\n' "$cf"
  done <<< "$chunks"
  ok "data loaded"

  # Sequences: the per-table split means the dump's own setval calls may not land.
  # Reset every identity/serial sequence to its column's true max, or the first
  # insert after cutover collides with a restored id. Done entirely server-side.
  arm_sql "$STAGING" <<'SEQ_SQL' || warn "sequence reset reported an error — check before swapping"
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT quote_ident(sn.nspname)||'.'||quote_ident(s.relname) AS seq,
           quote_ident(a.attname)                               AS col,
           quote_ident(n.nspname)||'.'||quote_ident(c.relname)  AS tbl
    FROM pg_class s
    JOIN pg_namespace sn ON sn.oid = s.relnamespace
    JOIN pg_depend d    ON d.objid = s.oid
                       AND d.classid = 'pg_class'::regclass
                       AND d.deptype IN ('a','i')
    JOIN pg_class c     ON c.oid = d.refobjid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = d.refobjsubid
    WHERE s.relkind = 'S'
  LOOP
    EXECUTE format('SELECT setval(%L, COALESCE((SELECT MAX(%s) FROM %s), 1), true)',
                   r.seq, r.col, r.tbl);
  END LOOP;
END $$;
SEQ_SQL
  ok "sequences reset to max(id)"

  # post-data lives in its own dump; build indexes/constraints in parallel.
  SRC_PW="${SRC_PW:-$(src_pw)}"
  pgtool "$SRC_PW" pg_dump -h "$SRC_HOST" -p "$SRC_PORT" -U "$SRC_USER" -d "$SRC_DB" \
    --section=post-data -Fc -Z 0 --no-owner --no-privileges -f /dumps/post.dump \
    || die "post-data dump failed"
  log "building indexes and constraints with -j ${RESTORE_JOBS} (this is the long part)"
  # FK constraints lock BOTH tables (ShareRowExclusiveLock); two workers taking the
  # same pair in opposite order deadlock under -j, so FKs restore serially after.
  printf 'docker run --rm -v %q:/dumps %s pg_restore -l /dumps/post.dump > %q/post.toc && grep -v "FK CONSTRAINT" %q/post.toc > %q/post.nofk.toc && grep "FK CONSTRAINT" %q/post.toc > %q/post.fk.toc\n' \
    "$DUMP_DIR" "$PG_IMAGE" "$DUMP_DIR" "$DUMP_DIR" "$DUMP_DIR" "$DUMP_DIR" "$DUMP_DIR" | arm \
    || die "post-data TOC split failed"
  pgtool "$ARM_PW" pg_restore -h 127.0.0.1 -p "$ARM_PORT_REMOTE" -U "$ARM_USER" -d "$STAGING" \
    --exit-on-error --no-owner --no-privileges -j "$RESTORE_JOBS" \
    --use-list=/dumps/post.nofk.toc /dumps/post.dump \
    || die "post-data restore failed (indexes/constraints pass)"
  pgtool "$ARM_PW" pg_restore -h 127.0.0.1 -p "$ARM_PORT_REMOTE" -U "$ARM_USER" -d "$STAGING" \
    --exit-on-error --no-owner --no-privileges --use-list=/dumps/post.fk.toc /dumps/post.dump \
    || die "post-data restore failed (FK pass)"
  ok "indexes and constraints built"
  printf 'docker exec %s psql -U %s -d %q -qc "ANALYZE;"\n' "$ARM_CONTAINER" "$ARM_USER" "$STAGING" | arm
  ok "analyzed"
}

# ── verify ────────────────────────────────────────────────────────────────────
cmd_verify() {
  [[ -s "$SRC_COUNTS" ]] || die "no source snapshot — run 'dump' first"
  log "verifying staging against the source snapshot"
  local fail=0 t want got
  while read -r t want; do
    [[ -z "$t" ]] && continue
    got="$(arm_q "$STAGING" "SELECT count(*) FROM ${t}" 2>/dev/null)"
    if [[ "$got" == "$want" ]]; then printf '    %-45s %s\n' "$t" "$got"
    else printf '%s    %-45s want %s got %s%s\n' "$c_e" "$t" "$want" "${got:-<missing>}" "$c_0"; fail=1; fi
  done < "$SRC_COUNTS"

  local inv; inv="$(arm_q "$STAGING" "SELECT count(*) FROM pg_index WHERE NOT indisvalid")"
  [[ "${inv:-0}" == 0 ]] && ok "no invalid indexes" || { warn "${inv} invalid index(es)"; fail=1; }

  local se; se="$(src_q "$SRC_DB" "SELECT string_agg(extname,',' ORDER BY extname) FROM pg_extension")"
  local te; te="$(arm_q "$STAGING" "SELECT string_agg(extname,',' ORDER BY extname) FROM pg_extension")"
  [[ "$se" == "$te" ]] && ok "extensions match ($te)" || { warn "extensions differ: source[$se] target[$te]"; fail=1; }

  if (( fail == 0 )); then mkdir -p "$STATE_LOCAL"; date -u +%FT%TZ > "$VERIFY_OK"; log "verify PASS"; return 0
  else rm -f "$VERIFY_OK"; die "verify FAILED — staging is NOT swapped in; nothing live was touched"; fi
}

# ── swap ──────────────────────────────────────────────────────────────────────
cmd_swap() {
  [[ -s "$VERIFY_OK" ]] || die "refusing to swap: verify has not passed"
  local ts; ts="$(date -u +%Y%m%d%H%M%S)"
  log "atomic rename swap (staging -> live)"
  # Renames are near-instant, so the unavailability window is milliseconds. Any
  # existing live DB is kept as a timestamped backup rather than dropped.
  arm_sql postgres <<SWAP_SQL || die "swap failed"
SELECT pg_terminate_backend(pid) FROM pg_stat_activity
 WHERE datname IN ('${SRC_DB}', '${STAGING}') AND pid <> pg_backend_pid();
DO \$\$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_database WHERE datname = '${SRC_DB}') THEN
    EXECUTE 'ALTER DATABASE ${SRC_DB} RENAME TO ${SRC_DB}__backup_${ts}';
  END IF;
END \$\$;
ALTER DATABASE ${STAGING} RENAME TO ${SRC_DB};
SWAP_SQL
  ok "live database on armitage is now '${SRC_DB}' (previous kept as ${SRC_DB}__backup_${ts} if it existed)"
}

# ── LAN exposure (Windows portproxy) ──────────────────────────────────────────
# Postgres runs in a Docker container inside WSL2, and WSL2's NAT hides it from
# the LAN. An elevated `netsh portproxy` rule on Windows bridges LAN:5440 to
# <wsl-ip>:5440 — the same mechanism already carrying neo4j, ollama and the rest
# on that box. The rule pins the WSL IP, which moves whenever the distro
# restarts, so `lan-fix` re-detects the IP and rewrites the rule.
ARM_PS='/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe'
ARM_PS1_WIN='C:\Users\fifth\open-5440.ps1'
ARM_PS1_WSL='/mnt/c/Users/fifth/open-5440.ps1'
ARM_LOG_WSL='/mnt/c/Users/fifth/open-5440.log'

# Is the DB reachable over the LAN, and if not, say why rather than just failing.
cmd_lan_check() {
  if nc -z -G 5 "$ARM_IP" "$ARM_PORT" >/dev/null 2>&1; then
    ok "armitage ${ARM_IP}:${ARM_PORT} reachable from this Mac"
    return 0
  fi
  warn "armitage ${ARM_IP}:${ARM_PORT} is NOT reachable"
  local wsl_ip rule_ip
  wsl_ip="$(ssh -o ConnectTimeout=8 "$ARM_SSH" 'hostname -I' 2>/dev/null | awk '{print $1}')"
  rule_ip="$(ssh -o ConnectTimeout=8 "$ARM_SSH" "$ARM_PS -NoProfile -Command 'netsh interface portproxy show v4tov4'" 2>/dev/null \
             | tr -d '\r' | awk -v p="$ARM_PORT_REMOTE" '$2==p {print $3}')"
  printf '  WSL IP now            : %s\n' "${wsl_ip:-<armitage unreachable over ssh>}"
  printf '  portproxy %s points : %s\n' "$ARM_PORT_REMOTE" "${rule_ip:-<no rule>}"
  if [[ -z "$wsl_ip" ]]; then
    warn "armitage is not answering ssh — is the machine up and the distro started?"
  elif [[ -z "$rule_ip" || "$wsl_ip" != "$rule_ip" ]]; then
    warn "the portproxy rule is missing or points at a stale WSL IP — run '$0 lan-fix'"
  else
    warn "the rule looks right, so check the container: ssh $ARM_SSH docker ps | grep $ARM_CONTAINER"
  fi
  return 1
}

# Install or repair the portproxy + firewall rule. armitage's UAC is set to
# elevate administrators without prompting, so this runs unattended; the elevated
# helper writes its own transcript, which we read back to confirm what happened.
cmd_lan_fix() {
  log "staging the portproxy helper on armitage"
  ssh -o ConnectTimeout=10 "$ARM_SSH" "cat > $ARM_PS1_WSL" <<'PS1EOF'
# Expose the mike capture DB (armitage-pg, WSL2 Docker :5440) to the LAN.
# Run elevated. Idempotent - safe to re-run, and MUST be re-run after a WSL
# restart, because the WSL IP the portproxy rule points at drifts.
Start-Transcript -Path 'C:\Users\fifth\open-5440.log' -Force | Out-Null
$ErrorActionPreference = 'Stop'
try {
    $wslIp = ((wsl.exe -- hostname -I) -join ' ').Trim().Split(' ')[0]
    if (-not $wslIp) { throw 'Could not determine the WSL IP.' }
    Write-Host "WSL IP: $wslIp"

    netsh interface portproxy delete v4tov4 listenport=5440 listenaddress=0.0.0.0 2>&1 | Out-Null
    netsh interface portproxy add v4tov4 listenport=5440 listenaddress=0.0.0.0 connectport=5440 connectaddress=$wslIp
    Write-Host "portproxy: 0.0.0.0:5440 -> ${wslIp}:5440"

    $rule = 'WSL2 Mike Capture DB'
    if (Get-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue) {
        Write-Host "firewall rule '$rule' already present"
    } else {
        New-NetFirewallRule -DisplayName $rule -Direction Inbound -Action Allow -Protocol TCP -LocalPort 5440 -Profile Private | Out-Null
        Write-Host "firewall rule '$rule' created (TCP 5440, Private profile)"
    }

    netsh interface portproxy show v4tov4 | Select-String 5440
    Write-Host 'RESULT: OK'
} catch {
    Write-Host "RESULT: FAILED - $_"
}
Stop-Transcript | Out-Null
PS1EOF
  ssh -o ConnectTimeout=10 "$ARM_SSH" "rm -f $ARM_LOG_WSL" >/dev/null 2>&1
  log "elevating on armitage and rewriting the rule"
  ssh -o ConnectTimeout=10 "$ARM_SSH" \
    "$ARM_PS -NoProfile -Command \"Start-Process powershell.exe -Verb RunAs -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File $ARM_PS1_WIN'\"" \
    || die "could not launch the elevated helper on armitage"
  local i
  for i in $(seq 1 24); do
    ssh -o ConnectTimeout=8 "$ARM_SSH" "grep -q 'RESULT:' $ARM_LOG_WSL 2>/dev/null" && break
    sleep 5
  done
  ssh -o ConnectTimeout=8 "$ARM_SSH" "cat $ARM_LOG_WSL 2>/dev/null" | tr -d '\r' \
    | grep -E 'WSL IP|portproxy:|firewall rule|RESULT' \
    || die "the elevated helper produced no result — check that 'fifth' is still an administrator on armitage"
  cmd_lan_check
}

# ── cutover / rollback ────────────────────────────────────────────────────────
repoint() { # repoint <host:port label>
  local host="$1" label="$2"
  cp "${PROJECT_ROOT}/.env" "${PROJECT_ROOT}/.env.bak.$(date -u +%Y%m%d%H%M%S)"
  cp "${PROJECT_ROOT}/.cargo/config.toml" "${PROJECT_ROOT}/.cargo/config.toml.bak.$(date -u +%Y%m%d%H%M%S)"
  sed -i '' -E "s#(@)[^/]+(/${SRC_DB}\"?)#\1${host}\2#" "${PROJECT_ROOT}/.env"
  sed -i '' -E "s#(@)[^/]+(/postgres)#\1${host}\2#" "${PROJECT_ROOT}/.env"
  sed -i '' -E "s#(@)[^/]+(/postgres)#\1${host}\2#" "${PROJECT_ROOT}/.cargo/config.toml"
  ok "config repointed at ${label} (.env + .cargo/config.toml; .bak files written)"
  grep -nE 'MIKE_DATABASE_URL|MIKE_TEST_ADMIN_URL' "${PROJECT_ROOT}/.env" | sed 's/:[^:@]*@/:***@/'
}

# Open (or confirm) the SSH tunnel that carries the DB link. Idempotent.
cmd_tunnel() {
  if nc -z -G 3 127.0.0.1 "$TUNNEL_PORT" >/dev/null 2>&1; then
    ok "tunnel already up on 127.0.0.1:${TUNNEL_PORT}"; return 0
  fi
  ssh -f -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 -o ServerAliveCountMax=4 \
      -L "${TUNNEL_PORT}:127.0.0.1:${ARM_PORT_REMOTE}" "$ARM_SSH" \
    || die "could not open the tunnel to ${ARM_SSH}"
  sleep 2
  nc -z -G 3 127.0.0.1 "$TUNNEL_PORT" >/dev/null 2>&1 \
    && ok "tunnel up: 127.0.0.1:${TUNNEL_PORT} -> ${ARM_SSH}:${ARM_PORT_REMOTE}" \
    || die "tunnel opened but the port is not answering"
  warn "the tunnel must be running whenever mike is — re-run 'tunnel' after a WSL or network restart"
}

cmd_cutover() {
  nc -z -G 5 "$ARM_IP" "$ARM_PORT" >/dev/null 2>&1 \
    || die "${ARM_IP}:${ARM_PORT} is not answering — run '$0 lan-fix' first"
  ok "target reachable at ${ARM_IP}:${ARM_PORT}"
  repoint "${ARM_IP}:${ARM_PORT}" "armitage"
  log "next: ./scripts/start-proxy.sh, then verify a REAL captured request — /api/health reported ok"
  log "      throughout the 2026-08-17 outage while capture was silently dropping."
}

cmd_rollback() { repoint "infra.local:${SRC_PORT}" "infra (Pi)"; }

# ── status ────────────────────────────────────────────────────────────────────
cmd_status() {
  printf '%s── migration status ──%s\n' "$c_i" "$c_0"
  if [[ -s "$SRC_COUNTS" ]]; then
    printf '  source snapshot : %s tables, %s rows total\n' \
      "$(wc -l < "$SRC_COUNTS" | tr -d ' ')" "$(awk -F'\t' '{s+=$2} END{print s}' "$SRC_COUNTS")"
  else printf '  source snapshot : (not taken)\n'; fi

  local done_units; done_units="$(printf 'grep -c "\tdone\t" %q 2>/dev/null || echo 0\n' "$MANIFEST" | arm)"
  local expect_units=0
  [[ -s "$SRC_COUNTS" ]] && expect_units=$(( $(wc -l < "$SRC_COUNTS" | tr -d ' ') + 1 ))
  printf '  dump units done : %s (tables+pre; chunks counted separately)\n' "${done_units:-0}"
  printf '  dump size       : %s\n' "$(printf 'du -sh %q 2>/dev/null | cut -f1\n' "$DUMP_DIR" | arm)"

  local staging_sz; staging_sz="$(arm_q postgres "SELECT pg_size_pretty(pg_database_size('${STAGING}'))" 2>/dev/null)"
  printf '  staging DB      : %s\n' "${staging_sz:-<none>}"
  local live_sz; live_sz="$(arm_q postgres "SELECT pg_size_pretty(pg_database_size('${SRC_DB}'))" 2>/dev/null)"
  printf '  live on armitage: %s\n' "${live_sz:-<none>}"
  printf '  verify          : %s\n' "$([[ -s "$VERIFY_OK" ]] && cat "$VERIFY_OK" || echo 'not passed')"
  printf '  repo points at  : %s\n' "$(sed -nE 's#^MIKE_DATABASE_URL="?postgres://[^@]+@([^/]+)/.*#\1#p' "${PROJECT_ROOT}/.env" | head -1)"
  nc -z -G 3 "$ARM_IP" "$ARM_PORT" >/dev/null 2>&1 \
    && printf '  armitage:%s    : reachable\n' "$ARM_PORT" \
    || printf '  armitage:%s    : NOT reachable — run %s lan-fix\n' "$ARM_PORT" "$0"
}

# ── main ──────────────────────────────────────────────────────────────────────
case "${1:-}" in
  preflight) cmd_preflight ;;
  lan-check) cmd_lan_check ;;
  lan-fix)   cmd_lan_fix ;;
  tunnel)    cmd_tunnel ;;
  dump)      SRC_PW="$(src_pw)"; cmd_dump ;;
  restore)   SRC_PW="$(src_pw)"; cmd_restore ;;
  verify)    SRC_PW="$(src_pw)"; cmd_verify ;;
  swap)      cmd_swap ;;
  cutover)   cmd_cutover ;;
  rollback)  cmd_rollback ;;
  status)    cmd_status ;;
  run)       SRC_PW="$(src_pw)"; cmd_preflight && cmd_dump && cmd_restore && cmd_verify && cmd_swap \
               && log "done up to the swap. Run 'cutover' when armitage:${ARM_PORT} is reachable." ;;
  *) sed -n '2,40p' "$0"; exit 2 ;;
esac
