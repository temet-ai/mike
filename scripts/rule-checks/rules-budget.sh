#!/usr/bin/env bash
# rules-budget.sh — enforce the token budget on the ALWAYS-LOADED .claude/rules/ set.
#
# Claude Code loads a rule file at session start UNLESS it declares `paths:`
# frontmatter, in which case it loads only when Claude touches a matching file
# (code.claude.com/docs/en/memory, "Path-specific rules"). A rule still counts
# as always-loaded if any of its `paths:` globs is BROAD enough to match
# nearly every edit (exactly: **/*.md, **/*, **) — such a glob defeats the
# point of scoping. The standing per-request tax is therefore the sum of the
# ALWAYS-LOADED (unscoped, or broad-scoped) rules only; narrowly path-scoped
# rules are deferred and billed per-domain-session.
#
# This gate counts ONLY the always-loaded set against RULES_BUDGET_TOKENS
# (default 10000). Path-scoped rules are reported for transparency (with a
# heavy-rule advisory) but not gated. To bring a rule under the gate, give it
# `paths:` frontmatter so it defers — or trim the always-loaded doctrine.
#
# Token estimate: real tokenizer counts are not available offline, so this uses a
# chars-per-token divisor CALIBRATED to the Claude Code `/context` "Memory files"
# reading (~2.58 chars/token for this dense markdown). Override the divisor with
# RULES_TOKEN_DIVISOR and the budget with RULES_BUDGET_TOKENS. The estimate is
# approximate by design; treat a result within ~5% of budget as "measure with a
# real tokenizer before trusting".
#
# Output: an always-loaded table (counted), a path-scoped table (deferred), the
# worst-case all-loaded total, and a PASS/FAIL verdict on the always-loaded set.
# Exit 0 = within budget, 1 = over budget, 2 = setup error.
set -uo pipefail

budget="${RULES_BUDGET_TOKENS:-10000}"
divisor="${RULES_TOKEN_DIVISOR:-2.58}"
# Advisory threshold: a deferred rule heavier than this loads a large chunk
# whenever its domain is touched — surfaced, not gated. Tunable; default chosen
# so today's heaviest rule (inspection-fidelity, ~5.4k) is flagged for awareness.
heavy="${RULES_HEAVY_DEFERRED_TOKENS:-4000}"

root="$(git rev-parse --show-toplevel 2>/dev/null)" || { echo "not in a git repo" >&2; exit 2; }
dir="$root/.claude/rules"
[ -d "$dir" ] || { echo "no $dir" >&2; exit 2; }

# A rule is always-loaded UNLESS its YAML frontmatter carries a `paths:` key.
# (frontmatter = leading `---`; scan to the closing `---` for a `paths:` line.)
has_paths() {
  awk 'NR==1 && $0!="---"{exit 1}   # no frontmatter -> always-loaded
       NR==1{next}
       $0=="---"{exit 1}            # frontmatter closed, no paths -> always-loaded
       /^paths:/{exit 0}' "$1"      # paths key found -> path-scoped
}

# A `paths:` glob is BROAD when it matches nearly every edit, defeating the
# point of scoping; treat exactly these as broad: **/*.md, **/*, **. A rule
# carrying any broad glob counts as always-loaded even though it has `paths:`.
has_broad_glob() {
  awk 'NR==1 && $0!="---"{exit 1}
       NR==1{next}
       NR>1 && $0=="---"{exit 1}     # frontmatter closed, no broad glob found
       /^[[:space:]]*-/{
         line=$0
         gsub(/^[[:space:]]*-[[:space:]]*/, "", line)
         gsub(/"/, "", line)
         gsub(/[[:space:]]+$/, "", line)
         if (line=="**/*.md" || line=="**/*" || line=="**") exit 0
       }' "$1"
}

tok() { awk -v c="$1" -v d="$divisor" 'BEGIN{ printf "%d", c/d }'; }
row() { printf '%-46s %8d %8d\n' "$1" "$2" "$3"; }
rule() { printf '%-46s %8s %8s\n' "----------------------------------------------" "--------" "--------"; }

always_chars=0; defer_chars=0; on_disk_files=0
always_rows=""; defer_rows=""
for f in "$dir"/*.md; do
  [ -e "$f" ] || continue
  c=$(wc -c < "$f" | tr -d ' '); t=$(tok "$c"); n="${f##*/}"
  on_disk_files=$((on_disk_files + 1))
  if has_paths "$f" && ! has_broad_glob "$f"; then
    defer_chars=$((defer_chars + c)); defer_rows+="$(row "$n" "$c" "$t")"$'\n'
  else
    always_chars=$((always_chars + c)); always_rows+="$(row "$n" "$c" "$t")"$'\n'
  fi
done

always_tokens=$(tok "$always_chars")
defer_tokens=$(tok "$defer_chars")
grand=$(tok $((always_chars + defer_chars)))

printf '%-46s %8s %8s\n' "ALWAYS-LOADED (counted vs budget)" "CHARS" "~TOKENS"; rule
printf '%s' "$always_rows"; rule; row "  subtotal" "$always_chars" "$always_tokens"
echo
printf '%-46s %8s %8s\n' "PATH-SCOPED (deferred, not counted)" "CHARS" "~TOKENS"; rule
printf '%s' "$defer_rows"; rule; row "  subtotal" "$defer_chars" "$defer_tokens"
echo
echo "always-loaded budget: $budget tokens   (divisor ${divisor} chars/token, calibrated to /context)"
echo "worst-case all-loaded: ~${grand} tokens (only if one session touches every domain)"
echo "on-disk total: ~${grand} tokens across ${on_disk_files} rule files in $dir (all files, regardless of classification)"

while IFS= read -r line; do
  [ -z "$line" ] && continue
  name=$(printf '%s' "$line" | awk '{print $1}')
  ftok=$(printf '%s' "$line" | awk '{print $NF}')
  if [ "${ftok:-0}" -gt "$heavy" ]; then
    echo "  advisory: deferred rule '$name' ~${ftok} tokens — heavy whenever its domain is touched; consider trimming."
  fi
done <<< "$defer_rows"
echo

if [ "$always_tokens" -le "$budget" ]; then
  pct=$(awk -v t="$always_tokens" -v b="$budget" 'BEGIN{ printf "%d", t*100/b }')
  echo "PASS — always-loaded ~${always_tokens} tokens (${pct}% of ${budget})"
  exit 0
else
  over=$((always_tokens - budget))
  echo "FAIL — always-loaded ~${always_tokens} tokens, OVER by ~${over} (budget ${budget})"
  echo "       give a rule \`paths:\` frontmatter to defer it, or trim the always-loaded doctrine."
  exit 1
fi
