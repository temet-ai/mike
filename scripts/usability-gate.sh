#!/usr/bin/env bash
# usability-gate.sh — deterministic, API-free usability checks over a PR diff.
#
# Runs the executable usability self-checks (the rules are written into this
# script) against the lines a change ADDS to the frontend. It is the Tier-1 gate behind the LLM usability review:
# unambiguous, binding-rule violations fail the build; softer copy smells are
# reported as warnings (advisory, never fail).
#
# It does NOT reimplement the layout-shift or type checks — those have dedicated
# runners (frontend/src/lib/frontendArchitecture.guard.test.ts and svelte-check),
# which the workflow invokes separately. This script is the grep tier only.
#
# Usage:
#   scripts/usability-gate.sh [--base <ref>] [--head <ref>] [--summary <file>]
# Defaults: base = $GITHUB_BASE_REF or origin/main; head = HEAD.
# Exit: 0 if no errors (warnings allowed), 1 if any error-severity hit.

set -uo pipefail

BASE="${GITHUB_BASE_REF:-}"
HEAD="HEAD"
SUMMARY=""

while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE="$2"; shift 2 ;;
    --head) HEAD="$2"; shift 2 ;;
    --summary) SUMMARY="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Resolve a usable base ref. In Actions, GITHUB_BASE_REF is a bare branch name.
if [ -z "$BASE" ]; then
  if git rev-parse --verify -q origin/main >/dev/null; then BASE="origin/main"; else BASE="HEAD~1"; fi
elif git rev-parse --verify -q "origin/$BASE" >/dev/null; then
  BASE="origin/$BASE"
fi

MB="$(git merge-base "$BASE" "$HEAD" 2>/dev/null || echo "$BASE")"

# Added lines only (drop the +++ file header), restricted to frontend source.
ADDED="$(git diff "$MB..$HEAD" -- \
  'frontend/src/**/*.svelte' 'frontend/src/**/*.ts' 'frontend/src/**/*.css' \
  | grep -E '^\+' | grep -vE '^\+\+\+' || true)"

errors=0
warnings=0
report=""

emit() { report+="$1"$'\n'; }

# check <severity> <name> <egrep-pattern> <why>
check() {
  local sev="$1" name="$2" pat="$3" why="$4"
  local hits
  hits="$(printf '%s\n' "$ADDED" | grep -nE "$pat" || true)"
  if [ -n "$hits" ]; then
    local n; n="$(printf '%s\n' "$hits" | grep -c . )"
    if [ "$sev" = "error" ]; then
      errors=$((errors + n)); emit "### ❌ ERROR — $name ($n)"
    else
      warnings=$((warnings + n)); emit "### ⚠️  WARN — $name ($n)"
    fi
    emit "$why"$'\n'
    emit '```'
    emit "$(printf '%s\n' "$hits" | head -20)"
    emit '```'
  fi
}

# ── ERROR severity: binding-rule violations (gate the build) ──────────────────

# inspection-fidelity: no truncation of content surfaces.
check error "Content truncation on an inspection surface" \
  '\b(line-clamp|truncate|text-ellipsis)\b|\.slice\(0,|\.substring\(0,|\.substr\(0,|\[truncated\]' \
  "Inspection content must render in full — scroll/wrap, never clip. See .claude/rules/agent-behavior-and-data.md."

# tailwind-v4-pitfalls: bracket-only CSS var miscompiles to invalid CSS.
check error "Tailwind bg-[--foo] / text-[--foo] miscompile" \
  '(bg|text|border|ring|fill|stroke|shadow)-\[--[a-zA-Z]' \
  "v4 does NOT auto-wrap --foo in var(); use bg-[var(--foo)] or bg-(--foo). See docs/frontend/CONVENTIONS.md."

# ── WARN severity: copy/help smells (advisory, never fail) ────────────────────

check warn "Generic button label (verb+noun missing)" \
  '>[[:space:]]*(Submit|OK|Continue|Done|Yes|No)[[:space:]]*<' \
  "A button is a promise — name the outcome (Send payment, Delete 3 files). plain-language-microcopy."

check warn "Raw status code as user-facing copy" \
  'Error [0-9]{3}|status [0-9]{3}' \
  "Errors say what happened + the one fix, not a raw code. plain-language-microcopy."

check warn "Empty state says nothing" \
  '>[[:space:]]*(No data|Nothing here|Empty)[[:space:]]*<' \
  "An empty state must instruct + offer a next action. ui-coherence-state-audit / plain-language-microcopy."

check warn "Filler / marketing words" \
  '\b(powerful|seamless|simply|quick and easy)\b|\bplease\b' \
  "Cut filler; users scan. plain-language-microcopy."

# Native title tooltip on an inline HTML element (lowercase tag) or a hand-rolled
# role="tooltip". Deliberately does NOT match a bare `title="…"` line, which is
# almost always a component prop (e.g. <PageIntro title="Home">), not a tooltip.
check warn "Native title= attribute / hand-rolled tooltip" \
  '<[a-z][a-z0-9]*[^>]*[[:space:]]title="|role="tooltip"' \
  "Use the Tooltip/HelpHint primitives (keyboard + screen-reader correct). contextual-help-disclosure."

check warn "Possible first-load tour auto-launch" \
  'onMount\([^)]*tour|firstVisit.*tour' \
  "Tours are user-invoked, never auto-launched on first load. invokable-page-tour."

# ── Report ────────────────────────────────────────────────────────────────────

header="## Usability gate (deterministic)

Diff: \`${MB:0:12}..${HEAD}\` · **${errors} error(s)**, **${warnings} warning(s)**
"

if [ "$errors" -eq 0 ] && [ "$warnings" -eq 0 ]; then
  body="$header"$'\n'"✅ No deterministic usability issues in the changed frontend lines."
else
  body="$header"$'\n'"$report"
fi

printf '%s\n' "$body"
[ -n "$SUMMARY" ] && printf '%s\n' "$body" > "$SUMMARY"

if [ "$errors" -gt 0 ]; then
  echo "FAIL: $errors error-severity usability violation(s)." >&2
  exit 1
fi
exit 0
