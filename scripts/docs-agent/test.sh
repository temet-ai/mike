#!/usr/bin/env bash
# test.sh — run the docs-agent pytest suite via an ephemeral uv-managed venv,
# never a .venv inside the repo. Kills the create/delete .venv loop seen in
# session 5fbfac59 (venv created 4x, deleted 3x across reqs 83764-83922, each
# cycle forcing a fresh pip-install+pytest run just to keep `git status` clean
# afterward — see artifacts/2026-07-02/tool-call-efficiency/report.md P7).
#
# `uv run --with-requirements` builds/reuses an ephemeral env in uv's cache
# (~/.cache/uv or $UV_CACHE_DIR), NOT `<repo>/.venv` — so there is nothing to
# create or tear down in the worktree, and git status stays clean by
# construction rather than by manual cleanup.
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir" || { echo "cannot cd to $script_dir" >&2; exit 2; }

command -v uv >/dev/null 2>&1 || { echo "uv not found on PATH" >&2; exit 2; }
[ -f requirements.txt ] || { echo "no requirements.txt in $script_dir" >&2; exit 2; }
[ -d tests ] || { echo "no tests/ dir in $script_dir" >&2; exit 2; }

uv run --with-requirements requirements.txt -m pytest tests/ -q "$@"
status=$?

exit "$status"
