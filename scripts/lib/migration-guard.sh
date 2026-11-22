#!/usr/bin/env bash
# migration-guard.sh — does a mike binary embed exactly the migrations of the source it was
# built from? A build cache once restored a binary without its newest migration file (mbx keyed
# crate mike on the files rustc read, and sqlx's migrate! lists the folder without telling
# rustc), the deploy shipped it, and the database never got the migration. Every deploy path
# (deploy-proxy.sh on omit and the Mac, deploy-archive-build.sh for the ARM64 build,
# deploy-release-build before an install) runs migration_guard before anything goes live.
#
#   migration_versions_in_dir <dir>             versions of the up migrations in a folder
#   migration_versions_at_commit <repo> <sha>   the same, read from a commit with git
#   migration_versions_in_binary <bin>          what <bin> --list-embedded-migrations prints
#   <versions> | migration_guard <bin>          0 when the sets match; else says what differs
#
# Versions are the digits before the first `_` of each `.sql` file; `.down.sql` files are
# left out, as `mike --list-embedded-migrations` leaves out down migrations.

MIGRATION_GUARD_DIR="crates/mike-base/src/store/persistence/migrations"

migration_versions_from_names() {
    local name
    while IFS= read -r name; do
        name="${name##*/}"
        case "${name}" in
            *.down.sql) continue ;;
            [0-9]*_*.sql) printf '%s\n' "${name%%_*}" ;;
        esac
    done | LC_ALL=C sort -u
}

migration_versions_in_dir() {
    [[ -d "$1" ]] || { echo "migration-guard: no migrations folder at $1" >&2; return 1; }
    local entry
    for entry in "$1"/*; do
        if [[ -f "${entry}" ]]; then printf '%s\n' "${entry}"; fi
    done | migration_versions_from_names
}

migration_versions_at_commit() {
    local names
    names="$(git -C "$1" ls-tree --name-only "$2:${MIGRATION_GUARD_DIR}" 2>/dev/null)" || {
        echo "migration-guard: commit $2 has no ${MIGRATION_GUARD_DIR} in $1" >&2
        return 1
    }
    printf '%s\n' "${names}" | migration_versions_from_names
}

migration_versions_in_binary() {
    local out
    out="$("$1" --list-embedded-migrations 2>/dev/null)" || return 1
    printf '%s\n' "${out}" | grep -E '^[0-9]+$' | LC_ALL=C sort -u
}

migration_guard_summary() {
    local count
    count="$(printf '%s\n' "$1" | grep -c .)"
    printf '%s' "$1" | head -n 10 | tr '\n' ' '
    [[ "${count}" -le 10 ]] || printf '... (%s in all)' "${count}"
}

migration_guard() {
    local bin="$1" expected embedded missing extra
    expected="$(cat)"
    if [[ -z "${expected}" ]]; then
        echo "migration-guard: no migration files to compare ${bin} against" >&2
        return 1
    fi
    if ! embedded="$(migration_versions_in_binary "${bin}")" || [[ -z "${embedded}" ]]; then
        echo "migration-guard: ${bin} cannot list its embedded migrations (--list-embedded-migrations failed or printed none; a binary from before the flag?)" >&2
        return 1
    fi
    missing="$(LC_ALL=C comm -23 <(printf '%s\n' "${expected}") <(printf '%s\n' "${embedded}"))"
    extra="$(LC_ALL=C comm -13 <(printf '%s\n' "${expected}") <(printf '%s\n' "${embedded}"))"
    if [[ -z "${missing}" && -z "${extra}" ]]; then
        return 0
    fi
    echo "migration-guard: ${bin} does not embed the migrations of its source:" >&2
    [[ -z "${missing}" ]] || echo "  missing from the binary: $(migration_guard_summary "${missing}")" >&2
    [[ -z "${extra}" ]] || echo "  in the binary but not in the source: $(migration_guard_summary "${extra}")" >&2
    echo "  A build cache restored a stale compile of crate mike; rebuild with MIKE_DEPLOY_BUILD_CACHE=cargo, or clear the stale entry." >&2
    return 1
}
