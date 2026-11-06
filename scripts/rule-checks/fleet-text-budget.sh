#!/usr/bin/env bash
# fleet-text-budget.sh - the words every fleet seat must read, held to a ratchet.
#
# Each seat reads the fleet rules, its own role file (the observer its contract and
# inspector brief too), this repository's .claude/fleet/project.md and the brief mike
# writes for it; fleet_text.py counts them, the briefs' fixed text read from the Rust
# that builds them. This check fails when the total grows past the number in
# fleet-text-budget.baseline. fleet_text.py overlap is the companion check that fails
# when one seat reads the same instruction twice.
#
# The baseline only goes down: the fleet-rule-auditor agent lowers it after each cut.
# Raising it means editing the baseline file in the same commit, with the reason in
# the commit message, so growth is a decision someone made and not drift.
# mike-machinery.md and observer-retro.md are read on demand and are not counted, nor
# are the agents that run outside a fleet's seats (fleet-rule-auditor,
# fleet-capability-author).
#
# Exit 0 = within the baseline, 1 = over it, 2 = setup error.
set -uo pipefail

root="$(git rev-parse --show-toplevel 2>/dev/null)" || { echo "not in a git repo" >&2; exit 2; }
base_file="$root/scripts/rule-checks/fleet-text-budget.baseline"
[ -f "$base_file" ] || { echo "no $base_file" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 is not on PATH" >&2; exit 2; }
baseline=$(tr -dc '0-9' < "$base_file")

report=$(python3 -I "$root/scripts/rule-checks/fleet_text.py" words) || exit 2
printf '%s\n' "$report" | sed '$d'
total=$(printf '%s\n' "$report" | tail -1 | tr -dc '0-9')
echo
echo "total $total words; baseline $baseline"
if [ "$total" -le "$baseline" ]; then
  [ "$total" -lt "$baseline" ] && echo "under the baseline by $((baseline - total)): lower fleet-text-budget.baseline to $total in this commit"
  echo "PASS"
  exit 0
fi
echo "FAIL - the fleet text grew by $((total - baseline)) words. Cut as much as you add (the fleet-rule-auditor agent says what can go),"
echo "       or raise fleet-text-budget.baseline in this commit and say why in the commit message."
exit 1
