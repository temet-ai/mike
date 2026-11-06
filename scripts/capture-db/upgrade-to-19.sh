#!/usr/bin/env bash
# Upgrade a stopped PostgreSQL 17 capture cluster to PostgreSQL 19 with pg_upgrade --link.
#
#   scripts/capture-db/upgrade-to-19.sh <root> <old-subdir> <new-subdir> [--check|--clone]
#
# <root> is a host directory holding both data directories: the data files are linked or
# cloned, not copied, so both must sit under one mount. The upgrade takes seconds to
# minutes. The old cluster must not be running. --check runs pg_upgrade's compatibility
# checks only and changes nothing in the old cluster.
#
# Default --link: after a real run the old cluster stays usable until the new one is first
# started, and never again after that. --clone (btrfs, XFS): copy-on-write clones, so the
# old cluster stays intact and usable as a rollback.
set -euo pipefail

image="mike/pg-upgrade:17-to-19"
root=${1:?root dir holding both data dirs}
old_sub=${2:?old data subdir}
new_sub=${3:?new data subdir}
mode=${4:-}

old_dir="$root/$old_sub"
sudo_test() { docker run --rm -v "$root":/root_dir alpine sh -c "$1"; }
sudo_test "[ -f '/root_dir/$old_sub/PG_VERSION' ]" || { echo "upgrade-to-19: $old_dir is not a data directory" >&2; exit 2; }
[ "$(sudo_test "cat '/root_dir/$old_sub/PG_VERSION'")" = 17 ] || { echo "upgrade-to-19: $old_dir is not a PostgreSQL 17 cluster" >&2; exit 2; }
if sudo_test "[ -f '/root_dir/$old_sub/postmaster.pid' ]"; then
  echo "upgrade-to-19: $old_dir has a postmaster.pid; stop the old server first" >&2; exit 2
fi

check_flag=""
transfer="--link"
case "$mode" in
  --check) check_flag="--check" ;;
  --clone) transfer="--clone" ;;
  "") ;;
  *) echo "upgrade-to-19: unknown mode $mode" >&2; exit 2 ;;
esac

docker run --rm -u postgres \
  -v "$root":/upgrade \
  -w /upgrade \
  -e OLD="/upgrade/$old_sub" -e NEW="/upgrade/$new_sub" -e CHECK="$check_flag" -e TRANSFER="$transfer" \
  "$image" bash -euo pipefail -c '
    if [ ! -f "$NEW/PG_VERSION" ]; then
      mkdir -p "$NEW"; chmod 700 "$NEW"
      /usr/lib/postgresql/19/bin/initdb -D "$NEW" -U devuser \
        --encoding=UTF8 --locale-provider=libc --locale=en_US.utf8 --data-checksums
      cp "$OLD/pg_hba.conf" "$NEW/pg_hba.conf"
      cp "$OLD/postgresql.auto.conf" "$NEW/postgresql.auto.conf"
    fi
    cd "$NEW"
    /usr/lib/postgresql/19/bin/pg_upgrade \
      -b /usr/lib/postgresql/17/bin -B /usr/lib/postgresql/19/bin \
      -d "$OLD" -D "$NEW" -U devuser "$TRANSFER" --jobs 8 $CHECK
  '
