#!/usr/bin/env bash
# default-off-mutation-and-egress.sh — executable form of the
# .claude/rules/agent-behavior-and-data.md self-check.
# Flags default-ON toggles added under the modules that mutate traffic or send
# captured content off the machine. Test setup that enables a feature on purpose
# is a legitimate hit; review each one.
# Prints offending lines, or "CLEAN". Exit 0 = clean, 1 = hits found.
# Usage: default-off-mutation-and-egress.sh [git-diff-range]   (default: working tree vs HEAD)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2
paths=()
for m in feature_toggles steerer weasel_intercept proxy_tools compress read_dedup recall_injector \
         reminders reminder_on_token repeated_notices lesson_worker judge_worker memory_harvester embed_worker llm \
         compose_prompt lazy_sections workspace config observability; do
  paths+=("crates/mike/src/$m/*")
done
paths+=('*.toml')
hits=$(git diff ${1:-} -- "${paths[@]}" \
  | grep -nE '^\+.*(enabled:\s*true|enabled\s*=\s*true|default(s)? (to )?(on|enabled)|on by default|enabled by default)' \
  || true)
if [ -n "$hits" ]; then printf '%s\n' "$hits"; exit 1; fi
echo CLEAN
