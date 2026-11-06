#!/usr/bin/env bash
# G7 — Declared build targets must be tracked in git
#
# G3 catches a migration that was written but never committed. This is the
# same failure one layer up: a config file that NAMES a source file gets
# committed while the file it names does not, so the build is broken for
# everyone whose checkout is the committed state — and unbroken for whoever
# wrote it, because their working tree still has the file.
#
# It happened on 2026-08-29. `crates/mike/Cargo.toml` declared five `[[test]]`
# targets — tests/proxy.rs, tests/integration.rs, tests/e2e.rs, tests/cli.rs,
# tests/schema.rs — and only the sibling DIRECTORIES were tracked. The entry
# points themselves, plus tests/common/harness_completeness.rs, existed only
# untracked. `.config/nextest.toml` had the mirror-image split: its committed
# copy filtered on binary names the restructure had already removed. From a
# clean checkout of main:
#
#   cargo test                 error: can't find integration-test `cli` at path
#   cargo clippy --all-targets same
#   cargo nextest run          error: operator didn't match any binary names
#   make test / scripts/gate   inherit both
#
# Nothing caught it because `cargo build` and `cargo clippy` (no --all-targets)
# never resolve test target paths, and the author's own tree always has the
# files. Copying the untracked entry points into a fresh worktree does not
# recover it either: they reference further untracked modules, so the errors
# just move.
#
# What this grader checks, all from the git INDEX rather than the worktree —
# the index is what other people will get:
#
#   1. Every `path = ` under a [[test]]/[[bin]]/[[bench]]/[[example]] section of
#      every workspace Cargo.toml resolves to a tracked file.
#   2. Every `#[path = "..."]` module a tracked test entry point pulls in is
#      itself tracked.
#   3. Every `binary(NAME)` that .config/nextest.toml filters on is a target
#      name some Cargo.toml actually declares.
#
# Usage:
#   g7_declared_targets_tracked.sh                          audit mode (default)
#   g7_declared_targets_tracked.sh --commit-scope <cached|worktree>
#
# Audit mode grades the whole repo and blocks on any finding — the right answer
# for run_all_graders.sh, which is asking about the repo rather than one commit.
#
# --commit-scope grades one commit, and is what the hooks pass. It blocks only a
# commit that itself touches the declaration surface (a Cargo.toml, the nextest
# config, or a file under a tests/ directory). Anything else gets the finding as
# a non-blocking WARN. This is G3's lesson (mike#163): a tree-level FAIL freezes
# every session's unrelated commit behind one session's in-flight file, and the
# escape people reach for — moving somebody else's file — is worse than the
# problem.
#
# Exit: 0=PASS (or advisory-only under --commit-scope), 1=FAIL

set -uo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"
cd "$ROOT" || exit 1

commit_scope=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --commit-scope) commit_scope="${2:-cached}"; shift 2 ;;
        *) shift ;;
    esac
done
[[ "$commit_scope" == "worktree" || "$commit_scope" == "cached" ]] || commit_scope="${commit_scope:+cached}"

# Content of a tracked declaration file (a Cargo.toml, a tests/*.rs entry
# point, .config/nextest.toml): in --commit-scope cached the index is the
# commit, so this reads `git show :<path>` (which honours GIT_INDEX_FILE and
# so sees a private index too) rather than the worktree file. A shared
# checkout can have three sessions' uncommitted mod/path lines sitting in the
# SAME worktree file at once; reading it from disk means whichever lines
# happen to be there right now get checked against trackedness, and a
# commit that touches none of those lines still fails on someone else's
# in-flight addition. Worktree/audit mode is unchanged: git status is asking
# about the tree, so the tree is what it reads.
dcat() {
    local path="$1"
    if [ "$commit_scope" = "cached" ]; then
        git show ":$path" 2>/dev/null
    else
        cat "$path" 2>/dev/null
    fi
}

echo "=== G7: Declared build targets are tracked ==="
echo ""

# Is a path present in the git index? The index is the whole question here: a
# file that exists only in the worktree is exactly the failure being graded.
tracked() { git ls-files --error-unmatch -- "$1" >/dev/null 2>&1; }

# Every `path = "..."` belonging to a target section of one Cargo.toml, printed
# as a repo-relative path. Target sections are the `[[test]]`-style arrays plus
# `[lib]`/`[[bin]]`; any other table resets the state so a `path` key belonging
# to, say, a dependency is never mistaken for a target.
declared_paths() {
    local manifest="$1" dir
    dir="$(dirname "$manifest")"
    dcat "$manifest" | awk -v dir="$dir" '
        /^[[:space:]]*\[/ {
            in_target = ($0 ~ /^[[:space:]]*\[\[?(test|bin|bench|example)\]\]?[[:space:]]*$/ ||
                         $0 ~ /^[[:space:]]*\[lib\][[:space:]]*$/)
            next
        }
        in_target && /^[[:space:]]*path[[:space:]]*=/ {
            line = $0
            sub(/^[^"]*"/, "", line)
            sub(/".*$/, "", line)
            if (line != "") print dir "/" line
        }
    '
}

# Every target NAME one Cargo.toml declares, for the nextest cross-check.
declared_names() {
    dcat "$1" | awk '
        /^[[:space:]]*\[/ {
            in_target = ($0 ~ /^[[:space:]]*\[\[?(test|bin|bench|example)\]\]?[[:space:]]*$/)
            next
        }
        in_target && /^[[:space:]]*name[[:space:]]*=/ {
            line = $0
            sub(/^[^"]*"/, "", line)
            sub(/".*$/, "", line)
            if (line != "") print line
        }
    '
}

fail=0

# ── 1. Declared target paths ──────────────────────────────────────────────────
missing_paths=""
while IFS= read -r manifest; do
    [ -z "$manifest" ] && continue
    while IFS= read -r p; do
        [ -z "$p" ] && continue
        tracked "$p" && continue
        if [ -f "$p" ]; then
            missing_paths+="  !! $p  (exists locally, NOT tracked) — declared by $manifest"$'\n'
        else
            missing_paths+="  !! $p  (does not exist at all) — declared by $manifest"$'\n'
        fi
    done < <(declared_paths "$manifest")
done < <(git ls-files -- '*/Cargo.toml' 'Cargo.toml' 2>/dev/null)

# ── 2. #[path] modules pulled in by tracked test entry points ─────────────────
missing_mods=""
while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    entry_dir="$(dirname "$entry")"
    while IFS= read -r rel; do
        [ -z "$rel" ] && continue
        mod_path="$entry_dir/$rel"
        tracked "$mod_path" && continue
        if [ -f "$mod_path" ]; then
            missing_mods+="  !! $mod_path  (exists locally, NOT tracked) — used by $entry"$'\n'
        else
            missing_mods+="  !! $mod_path  (does not exist at all) — used by $entry"$'\n'
        fi
    done < <(dcat "$entry" | grep -ohE '#\[path[[:space:]]*=[[:space:]]*"[^"]+"\]' 2>/dev/null \
             | sed -E 's/.*"([^"]+)".*/\1/')
done < <(git ls-files -- '*/tests/*.rs' 2>/dev/null)

# ── 3. nextest binary filters name real targets ───────────────────────────────
stale_filters=""
NEXTEST_CFG=".config/nextest.toml"
if tracked "$NEXTEST_CFG"; then
    all_names="$(while IFS= read -r m; do declared_names "$m"; done \
                 < <(git ls-files -- '*/Cargo.toml' 'Cargo.toml' 2>/dev/null) | sort -u)"
    while IFS= read -r name; do
        [ -z "$name" ] && continue
        grep -qxF "$name" <<<"$all_names" && continue
        stale_filters+="  !! binary($name) — no Cargo.toml declares a target by that name"$'\n'
    done < <(dcat "$NEXTEST_CFG" | grep -oE 'binary\([A-Za-z0-9_-]+\)' 2>/dev/null \
             | sed -E 's/binary\((.*)\)/\1/' | sort -u)
fi

# ── Decide whether findings block THIS commit ─────────────────────────────────
blocking=1
scope_reason=""
if [ -n "$commit_scope" ]; then
    if [ "$commit_scope" = "cached" ]; then
        changed="$(git diff --cached --name-only 2>/dev/null)"
    else
        changed="$(git status --porcelain 2>/dev/null | sed 's/^...//')"
    fi
    if grep -qE '(^|/)Cargo\.toml$|^\.config/nextest\.toml$|(^|/)tests/' <<<"$changed"; then
        blocking=1
        scope_reason="it touches a Cargo.toml, the nextest config, or a tests/ file"
    else
        blocking=0
    fi
fi

label="FAIL"; [ "$blocking" -eq 0 ] && label="WARN"

if [ -n "$missing_paths" ]; then
    echo "$label — Cargo.toml declares a target whose file is not tracked:"
    printf '%s' "$missing_paths"
    echo "  A clean checkout cannot build these targets: cargo test and"
    echo "  cargo clippy --all-targets both fail to resolve the path."
    echo ""
    fail=1
fi

if [ -n "$missing_mods" ]; then
    echo "$label — a tracked test entry point pulls in an untracked module:"
    printf '%s' "$missing_mods"
    echo "  The entry point compiles for whoever wrote the module and for nobody else."
    echo ""
    fail=1
fi

if [ -n "$stale_filters" ]; then
    stale_count="$(printf '%s' "$stale_filters" | grep -c . )"
    echo "$label — .config/nextest.toml filters on $stale_count target(s) that no longer exist:"
    # A restructure invalidates the whole filter list at once, so print a
    # readable sample rather than sixty identical lines.
    printf '%s' "$stale_filters" | head -10
    [ "$stale_count" -gt 10 ] && echo "  ... and $((stale_count - 10)) more"
    echo "  cargo nextest run aborts with \"operator didn't match any binary names\","
    echo "  which takes make test and scripts/gate down with it."
    echo ""
    fail=1
fi

echo "────────────────────────────────────────"
if [ "$fail" -eq 0 ]; then
    echo "PASS: every declared target path, test module and nextest filter resolves to a tracked file"
    exit 0
fi

if [ "$blocking" -eq 0 ]; then
    echo "PASS (advisory): this commit does not touch the declaration surface, so it"
    echo "  cannot be what broke the build. The finding still needs landing by whoever"
    echo "  wrote the file — moving another session's in-flight file is not the fix."
    exit 0
fi

echo "FAIL: commit the files, or drop the declarations that name them."
[ -n "$scope_reason" ] && echo "  This commit is graded because $scope_reason."
echo ""
echo "  git add is the fix. A pathspec commit that stages the config change and"
echo "  skips the file it names is how this happens every time."
exit 1
