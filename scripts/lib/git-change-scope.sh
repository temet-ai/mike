#!/usr/bin/env bash
# git-change-scope.sh — "what will this commit actually contain?", shared by the
# commit graders so two gates never disagree about the same commit.
#
# Sourced, never executed. Callers set two variables before the first call:
#
#   GCS_ROOT    repo root (git rev-parse --show-toplevel)
#   GCS_STAGE_PATHS  optional, newline-separated. When the gated command stages
#               an EXPLICIT list of paths (`git add a b c && git commit`), the
#               commit will contain the index plus those paths and nothing else.
#               Setting this restricts the worktree half of the scan to them, so
#               another session's untracked file in the shared checkout is not
#               read as SQL this commit carries. Unset (e.g. `git add -A`) keeps
#               the previous wide behaviour.
#   GCS_SCOPE   cached   — the staged index vs HEAD IS the commit. The git-native
#                          commit-msg hook and a plain `git commit` with
#                          everything already staged.
#               worktree — the command stages files AFTER a PreToolUse hook has
#                          read the index (`git commit -a/-am/--all`,
#                          `git add … && git commit`), so the effective change is
#                          the union of the index, tracked worktree changes vs
#                          HEAD, and untracked files. Reading only the index
#                          there is the stage-then-read bypass.
#
# Provides:
#   gcs_changed_names <pathspec...>   changed file names, one per line (may repeat)
#   gcs_added_lines   <pathspec...>   '+'-prefixed ADDED lines of the effective diff
#   gcs_file_content  <path>          the content this commit will carry
#   gcs_sql_shaped [pathspec...]      prints why the commit is SQL-shaped, rc 0;
#                                     silent rc 1 when it is not. Extra pathspecs
#                                     (typically `:(exclude)<path>`) narrow every
#                                     scan — G3 excludes the very migrations it is
#                                     flagging, so "does this commit carry SQL"
#                                     means SQL OTHER than the file in question.
#
# `gcs_sql_shaped` is the single definition of "this commit touches the schema
# contract", so G3 (untracked migrations) and G6 (perf evidence) fire on exactly
# the same set of commits. Two definitions is how mike#163 became a deadlock: G3
# graded the whole tree while G6 graded the commit, and no commit satisfied both.

: "${GCS_ROOT:=}"
: "${GCS_SCOPE:=cached}"

# Intersect a path list with GCS_STAGE_PATHS when that allowlist is set.
_gcs_only_staged() {
    if [[ -z "${GCS_STAGE_PATHS:-}" ]]; then cat; return 0; fi
    local line keep
    while IFS= read -r line; do
        [[ -z "$line" ]] && continue
        keep=""
        while IFS= read -r allow; do
            [[ -z "$allow" ]] && continue
            # A staged pathspec may name a file or a directory prefix.
            if [[ "$line" == "$allow" || "$line" == "${allow%/}/"* ]]; then keep=1; break; fi
        done <<< "$GCS_STAGE_PATHS"
        [[ -n "$keep" ]] && printf '%s\n' "$line"
    done
    return 0
}

gcs_changed_names() {
    git -C "$GCS_ROOT" diff --cached --name-only --diff-filter=ACMR -- "$@" 2>/dev/null
    if [[ "$GCS_SCOPE" == "worktree" ]]; then
        { git -C "$GCS_ROOT" diff --name-only --diff-filter=ACMR HEAD -- "$@" 2>/dev/null
          git -C "$GCS_ROOT" ls-files -o --exclude-standard -- "$@" 2>/dev/null
        } | _gcs_only_staged
    fi
    return 0
}

gcs_added_lines() {
    git -C "$GCS_ROOT" diff --cached -U0 -- "$@" 2>/dev/null | grep -E '^\+' | grep -vE '^\+\+\+'
    if [[ "$GCS_SCOPE" == "worktree" ]]; then
        local tracked f
        tracked=$(git -C "$GCS_ROOT" diff --name-only --diff-filter=ACMR HEAD -- "$@" 2>/dev/null | _gcs_only_staged)
        if [[ -n "$tracked" ]]; then
            while IFS= read -r f; do
                [[ -n "$f" ]] && git -C "$GCS_ROOT" diff -U0 HEAD -- "$f" 2>/dev/null \
                    | grep -E '^\+' | grep -vE '^\+\+\+'
            done <<< "$tracked"
        fi
        while IFS= read -r f; do  # untracked: the whole file is added content
            [[ -n "$f" && -f "$GCS_ROOT/$f" ]] && sed 's/^/+/' "$GCS_ROOT/$f" 2>/dev/null
        done < <(git -C "$GCS_ROOT" ls-files -o --exclude-standard -- "$@" 2>/dev/null | _gcs_only_staged)
    fi
    return 0
}

gcs_file_content() {
    if [[ "$GCS_SCOPE" == "worktree" && -f "$GCS_ROOT/$1" ]]; then
        cat "$GCS_ROOT/$1" 2>/dev/null
    else
        git -C "$GCS_ROOT" show ":$1" 2>/dev/null
    fi
    return 0
}

# artifacts/ and perf-evidence/ hold measurement snapshots, not schema: a commit
# carrying only the EXPLAIN output db-perf-check wrote is evidence FOR a SQL
# change, never a SQL change of its own.
gcs_sql_shaped() {
    local sql rs sqlx
    sql="$(gcs_changed_names '*.sql' ':(exclude)artifacts/*' ':(exclude,glob)**/perf-evidence/**' "$@" \
           | LC_ALL=C sort -u | grep -v '^$' || true)"
    if [[ -n "$sql" ]]; then
        printf '.sql file(s) in this commit: %s\n' "$(tr '\n' ' ' <<<"$sql")"
        return 0
    fi
    sqlx="$(gcs_changed_names '.sqlx/*' "$@" | LC_ALL=C sort -u | grep -v '^$' || true)"
    if [[ -n "$sqlx" ]]; then
        printf 'sqlx offline query cache changed (a query!/query_as! signature moved): %s\n' \
               "$(tr '\n' ' ' <<<"$sqlx" )"
        return 0
    fi
    rs="$(gcs_added_lines '*.rs' ':(exclude)*/tests/*' ':(exclude)artifacts/*' ':(exclude,glob)**/perf-evidence/**' "$@" \
          | grep -E 'sqlx::query(_as|_scalar)?!?\(|query_as!|query_scalar!|query!\(' \
          | grep -vE '^\+\s*//' || true)"
    if [[ -n "$rs" ]]; then
        echo 'query macro / raw-query line(s) added or changed in non-test .rs'
        return 0
    fi
    return 1
}
