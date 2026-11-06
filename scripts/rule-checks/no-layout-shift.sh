#!/usr/bin/env bash
# no-layout-shift.sh — executable form of the .claude/rules/visual-design-and-ui.md
# self-check. Runs the consolidated frontend architecture guard, including the
# layout-stability assertions (fails if any state-scoped CSS rule sets padding /
# margin / font-weight / gap / a width-bearing border).
# Exit code is sb-test's: 0 = pass, non-zero = a guard violation.
set -uo pipefail
repo="$(git rev-parse --show-toplevel)"
exec "$repo/scripts/sb-test" --unit --skip-typecheck frontendArchitecture.guard.test.ts
