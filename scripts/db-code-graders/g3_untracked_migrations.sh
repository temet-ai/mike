#!/usr/bin/env bash
# G3 — Untracked migration files grader
#
# The simplest grader. Catches migration files that were applied to the DB
# but never committed to git. The 20260526 incident: 3 migration files were
# untracked at incident time, meaning cargo build passed but the schema was
# ahead of the committed code.
#
# An untracked migration is dangerous because:
# - `migration_checksums.rs` cannot catch it (not in _sqlx_migrations yet, or
#   the file doesn't exist in git to compare against)
# - `sqlx::migrate!()` picks up the file at compile time, applies it to the DB,
#   but another developer's `git pull` won't get it — their DB migration is behind
#
# Usage:
#   g3_untracked_migrations.sh                          audit mode (default)
#   g3_untracked_migrations.sh --commit-scope <cached|worktree>
#
# Audit mode grades the whole working TREE and blocks on any uncommitted
# migration — the right answer for run_all_graders.sh, which is asking about the
# repo rather than about one commit.
#
# --commit-scope grades a specific commit and is what the commit hooks pass. A
# tree-level FAIL there froze every session's unrelated commit behind one
# session's in-flight file, while G6 simultaneously forbade landing that file
# without perf evidence the down capture DB could not produce — no commit in the
# tree satisfied both (mike#163). So under --commit-scope an uncommitted
# migration blocks only a commit that is itself SQL-shaped; anything else gets
# the finding as a non-blocking WARN. A commit carrying no SQL cannot orphan a
# migration. A commit that itself carries a migration blocks only on untracked
# migrations with a LOWER timestamp; later ones are WARN, so seats that each own
# a migration commit independently, in either order (mike#522). "SQL-shaped" is scripts/lib/git-change-scope.sh's single definition,
# shared with G6, so the two gates can no longer disagree about one commit.
#
# Exit: 0=PASS (or advisory-only under --commit-scope), 1=FAIL

set -uo pipefail

CRATE_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"
MIGRATIONS_DIR="crates/mike-base/src/store/persistence/migrations"
INSPECT_URL="${MIKE_INSPECT_URL:-http://127.0.0.1:9989}"

commit_scope=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --commit-scope) commit_scope="${2:-cached}"; shift 2 ;;
        *) shift ;;
    esac
done
[[ "$commit_scope" == "worktree" || "$commit_scope" == "cached" ]] || commit_scope="${commit_scope:+cached}"

echo "=== G3: Untracked migration files ==="
echo ""

# Who wrote this file? `list_file_writes` is the only attribution source, and it
# is capture-DB-backed — the outage that triggers this grader is the same outage
# that silences attribution, so say which of the two happened rather than
# printing an unexplained blank (mike#163).
attribute() {
    local path="$1" base resp writes
    base="$(basename "$path")"
    if ! command -v curl >/dev/null 2>&1 || ! command -v jq >/dev/null 2>&1; then
        echo "     owner: not looked up — curl and jq are both needed to ask the write-attribution API"
        return
    fi
    resp="$(curl -sS -m 3 -G --data-urlencode "path=$base" "$INSPECT_URL/api/file-writes" 2>/dev/null)"
    if [[ -z "$resp" ]] || ! jq -e . >/dev/null 2>&1 <<<"$resp"; then
        echo "     owner: unattributable — $INSPECT_URL/api/file-writes did not answer."
        echo "            It is the only attribution source and it reads the capture DB, so a DB"
        echo "            outage takes attribution down with it. Ask in the workspace roster."
        return
    fi
    writes="$(jq -r --arg b "$base" '
        [ .files[]? | select(.file_path | endswith($b)) | .writes[]? ]
        | if length == 0 then empty
          else group_by(.session_id)
               | map({s: .[0].session_id,
                      tool: .[0].tool_name,
                      t: (map(.created_at) | min),
                      a: (map(.agent_name) | map(select(. != null)) | first)})
               | .[]
               | "     owner: session \(.s) — \(.tool) at \(.t)" +
                 (if .a then " (agent: \(.a))" else "" end)
          end' <<<"$resp" 2>/dev/null)"
    if [[ -n "$writes" ]]; then
        printf '%s\n' "$writes"
    else
        echo "     owner: no captured write names this file."
        jq -r '"            " + (.coverage // "")' <<<"$resp" 2>/dev/null
        echo "            A migration created with a Bash heredoc is unattributable by construction."
    fi
}

# Find .sql files in the migrations directory that are untracked or modified
UNTRACKED=$(git -C "$CRATE_ROOT" ls-files --others --exclude-standard \
    -- "$MIGRATIONS_DIR/*.sql" 2>/dev/null || true)

# `ls-files --modified` also reports files missing from the worktree, so
# deletions are subtracted out and graded separately below.
DELETED=$(git -C "$CRATE_ROOT" ls-files --deleted \
    -- "$MIGRATIONS_DIR/*.sql" 2>/dev/null || true)

MODIFIED=$(git -C "$CRATE_ROOT" ls-files --modified \
    -- "$MIGRATIONS_DIR/*.sql" 2>/dev/null || true)
if [ -n "$DELETED" ]; then
    MODIFIED=$(comm -23 <(echo "$MODIFIED" | sort -u) <(echo "$DELETED" | sort -u) | sed '/^$/d')
fi

STAGED=$(git -C "$CRATE_ROOT" diff --cached --name-only \
    -- "$MIGRATIONS_DIR/*.sql" 2>/dev/null || true)

# Under --commit-scope, decide up front what blocks this commit.
blocking=1
sql_reason=""
carried_min=""
UNTRACKED_LATER=""
mig_ts() { local b; b="$(basename "$1")"; b="${b%%_*}"; printf '%s' "${b//[!0-9]/}"; }
if [ -n "$commit_scope" ]; then
    GCS_LIB="$CRATE_ROOT/scripts/lib/git-change-scope.sh"
    if [ -f "$GCS_LIB" ] && source "$GCS_LIB" 2>/dev/null; then
        GCS_ROOT="$CRATE_ROOT"
        GCS_SCOPE="$commit_scope"
        # Migrations this commit carries. A peer's untracked migration with a
        # LATER timestamp cannot be orphaned by this commit's own migration, so
        # seats that each own a migration commit independently (mike#522); an
        # OLDER one still blocks, since this commit would land out of order.
        # Wide worktree scope (no explicit stage paths) counts every untracked
        # file as the commit's, so only the index is trusted there.
        if [ "$commit_scope" = "worktree" ] && [ -z "${GCS_STAGE_PATHS:-}" ]; then
            carried="$(git -C "$CRATE_ROOT" diff --cached --name-only --diff-filter=ACMR \
                -- "$MIGRATIONS_DIR/*.sql" 2>/dev/null | LC_ALL=C sort -u | grep -v '^$' || true)"
        else
            carried="$(gcs_changed_names "$MIGRATIONS_DIR/*.sql" | LC_ALL=C sort -u | grep -v '^$' || true)"
        fi
        if [ -n "$carried" ]; then
            while IFS= read -r f; do
                t="$(mig_ts "$f")"
                [ -z "$t" ] && continue
                if [ -z "$carried_min" ] || [ "$((10#$t))" -lt "$((10#$carried_min))" ]; then carried_min="$t"; fi
            done <<< "$carried"
            older=""
            while IFS= read -r f; do
                [ -z "$f" ] && continue
                grep -qxF "$f" <<<"$carried" && continue
                t="$(mig_ts "$f")"
                if [ -n "$carried_min" ] && [ -n "$t" ] && [ "$((10#$t))" -gt "$((10#$carried_min))" ]; then
                    UNTRACKED_LATER="${UNTRACKED_LATER}${f}"$'\n'
                else
                    older="${older}${f}"$'\n'
                fi
            done <<< "$UNTRACKED"
            UNTRACKED="${older%$'\n'}"
            UNTRACKED_LATER="${UNTRACKED_LATER%$'\n'}"
        fi
        # Exclude the flagged files themselves. The hazard is code landing while
        # its migration does not, so the question is whether the commit carries
        # SQL OTHER than the uncommitted migration — otherwise worktree scope
        # (which counts untracked files) reads a peer's in-flight migration as
        # this commit's own SQL and re-arms the deadlock it is meant to clear.
        gcs_excludes=()
        while IFS= read -r f; do
            [ -n "$f" ] && gcs_excludes+=(":(exclude)$f")
        done < <(printf '%s\n%s\n%s\n' "$UNTRACKED" "$UNTRACKED_LATER" "$MODIFIED")
        if sql_reason="$(gcs_sql_shaped "${gcs_excludes[@]}")"; then
            blocking=1
        else
            blocking=0
        fi
    else
        # Cannot tell what the commit contains — grade the tree, as before.
        sql_reason="scripts/lib/git-change-scope.sh did not load; graded tree-wide"
    fi
fi

label="FAIL"; marker="!!"
[ "$blocking" -eq 0 ] && { label="WARN"; marker="~~"; }

# An older untracked migration blocks whenever the commit carries a migration.
untracked_label="$label"; untracked_marker="$marker"
if [ -n "$carried_min" ]; then untracked_label="FAIL"; untracked_marker="!!"; fi

fail=0
hard=0

if [ -n "$UNTRACKED" ]; then
    echo "$untracked_label — Untracked migration files (applied to DB but not committed):"
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        echo "  $untracked_marker $f"
        # Try to show what the migration adds (first meaningful DDL line)
        local_path="$CRATE_ROOT/$f"
        if [ -f "$local_path" ]; then
            summary=$(grep -iE '^\s*(ALTER TABLE|CREATE TABLE|CREATE INDEX|DROP|INSERT|UPDATE)' \
                "$local_path" | head -3 | sed 's/^/     /')
            echo "$summary"
        fi
        attribute "$f"
    done <<< "$UNTRACKED"
    fail=1
    [ "$untracked_label" = "FAIL" ] && hard=1
    echo ""
fi

if [ -n "$UNTRACKED_LATER" ]; then
    echo "WARN — Untracked migrations with a later timestamp than this commit's (a peer's in-flight work; not blocking, mike#522):"
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        echo "  ~~ $f"
        attribute "$f"
    done <<< "$UNTRACKED_LATER"
    echo ""
fi

if [ -n "$MODIFIED" ]; then
    echo "$label — Modified (uncommitted) migration files:"
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        echo "  $marker $f"
        echo "  (editing an applied migration changes its checksum → proxy startup failure)"
        attribute "$f"
    done <<< "$MODIFIED"
    fail=1
    [ "$label" = "FAIL" ] && hard=1
    echo ""
fi

# Non-blocking: a file absent from the worktree carries no uncommitted schema
# change, which is the only hazard this grader exists to catch.
if [ -n "$DELETED" ]; then
    echo "WARN — Migration files deleted in the worktree but not staged:"
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        echo "  ~~~ $f"
    done <<< "$DELETED"
    echo "  (whoever deleted them owns staging it; if they were ever applied,"
    echo "   the DB that has them will fail startup with VersionMissing)"
    echo ""
fi

if [ -n "$STAGED" ]; then
    echo "INFO — Staged migration files (pending commit — OK if intentional):"
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        echo "  ~~~ $f"
    done <<< "$STAGED"
    echo ""
fi

echo "────────────────────────────────────────"
if [ "$fail" -eq 0 ] && [ -z "$UNTRACKED_LATER" ]; then
    echo "PASS: all migration files in $MIGRATIONS_DIR are committed"
    exit 0
fi

if [ "$hard" -eq 0 ] && [ -n "$UNTRACKED_LATER" ] && [ "$fail" -eq 0 ]; then
    echo "PASS (advisory): the untracked migrations above are later than this commit's own, so it cannot orphan them."
    exit 0
fi

if [ "$hard" -eq 0 ]; then
    echo "PASS (advisory): this commit carries no SQL, so it cannot orphan the migration above."
    echo "  The file still needs landing by whoever wrote it — it is not this commit's to fix,"
    echo "  and moving another session's in-flight file is not the escape (mike#163)."
    exit 0
fi

echo "FAIL: uncommitted migration files exist. Commit them or apply via a new migration."
if [ -n "$commit_scope" ] && [ -n "$sql_reason" ]; then
    echo "  This commit is graded because it is SQL-shaped: $sql_reason"
fi
echo ""
echo "Tip: if you intentionally wrote a migration without committing it,"
echo "  this is the precondition for the 20260526 incident."
echo "  Apply + commit atomically, or document why they're separate."
exit 1
