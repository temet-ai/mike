#!/usr/bin/env bash
# judgement-context-docs.sh — doc drift check for the judgement-context program.
# Prints CLEAN, or one line per finding: DRIFT <check> <item> <file>[:<line>] <reason>
# Exit 0 = clean, 1 = findings. Findings are never truncated or summarised.
#
# Inputs (all under --root, default the repo root):
#   scripts/rule-checks/judgement-context-docs.baseline   today's known drift
#   scripts/rule-checks/judgement-context-docs.d/*.ledger one file per package
#   migrations, TOGGLE_META, settings meta_*.rs, MCP tool constants, the docs
#
# Usage:
#   judgement-context-docs.sh [--root DIR] [--all-new [--strict]]
#   judgement-context-docs.sh --self-test
#   judgement-context-docs.sh --write-baseline [--force] [--root DIR]
set -euo pipefail

SELF="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/$(basename -- "${BASH_SOURCE[0]}")"
ROOT=""
ALL_NEW=0
STRICT=0
FORCE=0
MODE=check

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="${2:?--root needs a directory}"; shift 2 ;;
    --all-new) ALL_NEW=1; shift ;;
    --strict) STRICT=1; shift ;;
    --force) FORCE=1; shift ;;
    --self-test) MODE=selftest; shift ;;
    --write-baseline) MODE=baseline; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$ROOT" ]; then
  ROOT="$(cd -- "$(dirname -- "$SELF")/../.." && pwd)"
fi
cd "$ROOT"

MIG=crates/mike/src/store/persistence/migrations
TOGGLES=crates/mike/src/feature_toggles/mod.rs
# TOGGLE_META moved to meta.rs; older trees (and the fixtures) keep it in mod.rs.
TOGGLE_META_SRC=crates/mike/src/feature_toggles/meta.rs
CATALOGUE=crates/mike-mcp/src/tools/catalogue.rs
RC=scripts/rule-checks
BASELINE=$RC/judgement-context-docs.baseline
LEDGER_DIR=$RC/judgement-context-docs.d
SCHEMA_DOC=docs/SCHEMA.md
FEATURES_DOC=docs/FEATURES.md
MCP_DOC=docs/MCP_BRIDGE.md
MACHINERY=.claude/fleet/mike-machinery.md
OWNED_DOCS="docs/ARCHITECTURE.md docs/SCHEMA.md docs/CONFIGURATION.md docs/MCP_BRIDGE.md docs/FEATURES.md docs/SYSTEM_MAP.md"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FINDINGS=0
REVIEWS=0

migration_files() {
  [ -d "$MIG" ] || return 0
  find "$MIG" -maxdepth 1 -name '*.sql' | LC_ALL=C sort
}

list_tables() {
  local f
  for f in $(migration_files); do
    grep -HiE '^[[:space:]]*(CREATE TABLE|DROP TABLE)' "$f" || true
  done | awk '
    {
      line = $0
      sub(/^[^:]*:/, "", line)
      up = toupper(line)
      isdrop = (up ~ /DROP TABLE/)
      gsub(/"/, "", line)
      if (match(line, /[A-Za-z_][A-Za-z_0-9]*\.[A-Za-z_][A-Za-z_0-9]*/)) {
        t = tolower(substr(line, RSTART, RLENGTH))
        if (isdrop) delete seen[t]; else seen[t] = 1
      }
    }
    END { for (t in seen) print t }' | LC_ALL=C sort -u
}

list_relations() {
  {
    list_tables
    local f
    for f in $(migration_files); do
      grep -iE '^[[:space:]]*CREATE[[:space:]]+(OR[[:space:]]+REPLACE[[:space:]]+)?(MATERIALIZED[[:space:]]+)?VIEW' "$f" || true
    done | tr -d '"' | grep -oE '[A-Za-z_][A-Za-z_0-9]*\.[A-Za-z_][A-Za-z_0-9]*' | tr 'A-Z' 'a-z' || true
  } | LC_ALL=C sort -u
}

list_columns() {
  local f
  for f in $(migration_files); do
    awk '
      BEGIN { intable = 0 }
      {
        raw = $0
        gsub(/"/, "", raw)
        low = tolower(raw)
        if (!intable && low ~ /^[[:space:]]*create table/) {
          if (match(raw, /[A-Za-z_][A-Za-z_0-9]*\.[A-Za-z_][A-Za-z_0-9]*/)) {
            tbl = tolower(substr(raw, RSTART, RLENGTH))
            intable = 1
            rest = substr(raw, RSTART + RLENGTH)
            sub(/^[^(]*\(/, "", rest)
            if (rest != raw && rest ~ /[A-Za-z_]/) first_col(rest)
            next
          }
        }
        if (intable) {
          if (low ~ /^[[:space:]]*\)/) { intable = 0; next }
          first_col(raw)
          next
        }
        if (low ~ /alter table/ && low ~ /add column/) {
          if (match(raw, /[A-Za-z_][A-Za-z_0-9]*\.[A-Za-z_][A-Za-z_0-9]*/)) {
            t2 = tolower(substr(raw, RSTART, RLENGTH))
            r2 = tolower(raw)
            sub(/.*add column[[:space:]]+(if not exists[[:space:]]+)?/, "", r2)
            split(r2, parts, /[[:space:],]+/)
            if (parts[1] != "") print t2, parts[1]
          }
        }
      }
      function first_col(s,   w, l) {
        sub(/^[[:space:]]+/, "", s)
        l = tolower(s)
        if (l ~ /^(--|constraint|primary|unique|foreign|check|like|exclude)/) return
        if (l ~ /^(and|or|references|not|when|then|else)([[:space:](]|$)/) return
        split(s, w, /[[:space:],()]+/)
        if (w[1] ~ /^[A-Za-z_][A-Za-z_0-9]*$/) print tbl, tolower(w[1])
      }' "$f"
  done | LC_ALL=C sort -u
}

list_toggles() {
  local src="$TOGGLES"
  [ -f "$TOGGLE_META_SRC" ] && src="$TOGGLE_META_SRC"
  [ -f "$src" ] || return 0
  awk '/TOGGLE_META: / { on = 1 } on && /^\];/ { on = 0 } on' "$src" \
    | grep -oE 'key: "[^"]+"' | sed -E 's/key: "([^"]+)"/\1/' | LC_ALL=C sort -u || true
}

list_settings() {
  local f
  for f in crates/mike/src/settings/meta_*.rs; do
    [ -f "$f" ] || continue
    case "$f" in *_tests.rs) continue ;; esac
    grep -oE 'key: "[^"]+"' "$f" || true
  done | sed -E 's/key: "([^"]+)"/\1/' | LC_ALL=C sort -u
}

list_tool_consts() {
  [ -d crates/mike-mcp/src ] || return 0
  grep -rhoE 'pub const TOOL_[A-Z0-9_]+: &str = "[^"]+"' crates/mike-mcp/src \
    | sed -E 's/pub const (TOOL_[A-Z0-9_]+): &str = "([^"]+)"/\1 \2/' | LC_ALL=C sort -u || true
}

list_tools() { list_tool_consts | awk '{ print $2 }' | LC_ALL=C sort -u; }

list_core_tools() {
  [ -f "$CATALOGUE" ] || return 0
  list_tool_consts > "$TMP/consts"
  awk -v consts="$TMP/consts" '
    BEGIN { while ((getline l < consts) > 0) { split(l, p, " "); c[p[1]] = p[2] } }
    /CORE_TOOL_NAMES:/ { on = 1; next }
    on && /^\];/ { on = 0 }
    on {
      if (match($0, /"[^"]+"/)) print substr($0, RSTART + 1, RLENGTH - 2)
      else if (match($0, /TOOL_[A-Z0-9_]+/)) { k = substr($0, RSTART, RLENGTH); if (k in c) print c[k] }
    }' "$CATALOGUE" | LC_ALL=C sort -u
}

in_baseline() { [ -f "$BASELINE" ] && grep -qxF -- "$1" "$BASELINE"; }

report() {
  local label="$1"; shift
  printf '%s %s\n' "$label" "$*"
  if [ "$label" = DRIFT ]; then FINDINGS=$((FINDINGS + 1)); else REVIEWS=$((REVIEWS + 1)); fi
}

section_of() {
  awk -v h="### \`$1\`" '
    index($0, h) == 1 { on = 1; print; next }
    on && /^##/ { on = 0 }
    on' "$SCHEMA_DOC"
}

check_table() {
  local t="$1" label="$2" section col
  if ! grep -qE "^### \`$t\`" "$SCHEMA_DOC" 2>/dev/null; then
    report "$label" "C1 $t $SCHEMA_DOC no heading \"### \`$t\`\""
    return
  fi
  section="$(section_of "$t")"
  awk -v t="$t" '$1 == t { print $2 }' "$TMP/columns" | while read -r col; do
    if ! printf '%s\n' "$section" | awk -F'|' -v c="$col" '
        /^[[:space:]]*\|/ { x = $2; gsub(/[[:space:]`]/, "", x); if (x == c) f = 1 }
        END { exit !f }'; then
      echo "$col"
    fi
  done > "$TMP/missing_cols"
  while read -r col; do
    report "$label" "C1 $t $SCHEMA_DOC column \"$col\" not in the section"
  done < "$TMP/missing_cols"
}

check_toggle() {
  local k="$1" label="$2"
  if ! list_toggles | grep -qxF -- "$k"; then
    report "$label" "C2 $k ${TOGGLE_META_SRC} not in TOGGLE_META"
  fi
  if ! grep -qF -- "\`$k\`" "$FEATURES_DOC" 2>/dev/null; then
    report "$label" "C2 $k $FEATURES_DOC toggle not written as \`$k\`"
  fi
}

check_setting() {
  local k="$1" label="$2"
  if ! grep -qF -- "$k" "$FEATURES_DOC" docs/ARCHITECTURE.md docs/CONFIGURATION.md 2>/dev/null; then
    report "$label" "C3 $k docs/FEATURES.md,docs/ARCHITECTURE.md,docs/CONFIGURATION.md setting key absent"
  fi
}

check_tool() {
  local name="$1" profile="$2" label="$3" in_core=0 full_section
  if list_core_tools | grep -qxF -- "$name"; then in_core=1; fi
  if ! grep -qF -- "\`$name\`" "$MCP_DOC" 2>/dev/null; then
    report "$label" "C4 $name $MCP_DOC tool absent"
    return
  fi
  full_section="$(awk '/^### Full-only tools/ { on = 1; next } on && /^###/ { on = 0 } on' "$MCP_DOC")"
  if [ "$profile" = core ] && printf '%s\n' "$full_section" | grep -qF -- "\`$name\`"; then
    report "$label" "C4 $name $MCP_DOC core tool listed under Full-only tools"
  fi
  if [ "$profile" = core ] && [ "$in_core" = 0 ]; then
    report "$label" "C4 $name $CATALOGUE ledger says core, CORE_TOOL_NAMES does not list it"
  fi
  if [ "$profile" = full ] && [ "$in_core" = 1 ]; then
    report "$label" "C4 $name $CATALOGUE ledger says full, CORE_TOOL_NAMES lists it"
  fi
}

check_cli() {
  local words="$1"
  if ! grep -qF -- "$words" docs/CONFIGURATION.md docs/STARTUP_FLOW.md 2>/dev/null; then
    report DRIFT "C5 $words docs/CONFIGURATION.md,docs/STARTUP_FLOW.md verb absent"
  fi
}

trim() { local s="$1"; s="${s#"${s%%[![:space:]]*}"}"; printf '%s' "${s%"${s##*[![:space:]]}"}"; }

check_fragment() {
  local kind="$1" pkg="$2" file="$3" frag="$4" ln
  grep -qE "^landed[[:space:]]+$pkg[[:space:]]" "$TMP/ledger_all" || return 0
  if [ ! -f "$file" ]; then
    report DRIFT "C6 $pkg $file file missing for $kind fragment \"$frag\""
    return
  fi
  ln="$(grep -nF -m1 -- "$frag" "$file" | cut -d: -f1 || true)"
  if [ "$kind" = retire ] && [ -n "$ln" ]; then
    report DRIFT "C6 $pkg $file:$ln retired fragment still present: $frag"
  elif [ "$kind" = require ] && [ -z "$ln" ]; then
    report DRIFT "C6 $pkg $file required fragment absent: $frag"
  fi
}

process_ledgers() {
  local f pkg kind rest a b
  : > "$TMP/ledger_all"
  : > "$TMP/ledger_tables"; : > "$TMP/ledger_toggles"; : > "$TMP/ledger_settings"; : > "$TMP/ledger_tools"
  [ -d "$LEDGER_DIR" ] || return 0
  for f in $(find "$LEDGER_DIR" -maxdepth 1 -name '*.ledger' | LC_ALL=C sort); do
    pkg="$(basename "$f" .ledger)"
    while IFS= read -r line || [ -n "$line" ]; do
      line="$(trim "$line")"
      case "$line" in ''|'#'*) continue ;; esac
      read -r kind rest <<< "$line"
      case "$kind" in
        landed) printf 'landed %s %s\n' "$pkg" "$rest" >> "$TMP/ledger_all" ;;
        table) printf '%s\n' "$(trim "$rest")" >> "$TMP/ledger_tables" ;;
        toggle) printf '%s\n' "$(trim "$rest")" >> "$TMP/ledger_toggles" ;;
        setting) printf '%s\n' "$(trim "$rest")" >> "$TMP/ledger_settings" ;;
        tool) read -r a b <<< "$rest"; printf '%s %s\n' "$a" "${b:-core}" >> "$TMP/ledger_tools" ;;
      esac
    done < "$f"
  done
  for f in $(find "$LEDGER_DIR" -maxdepth 1 -name '*.ledger' | LC_ALL=C sort); do
    pkg="$(basename "$f" .ledger)"
    while IFS= read -r line || [ -n "$line" ]; do
      line="$(trim "$line")"
      case "$line" in ''|'#'*) continue ;; esac
      read -r kind rest <<< "$line"
      case "$kind" in
        table) check_table "$(trim "$rest")" DRIFT ;;
        toggle) check_toggle "$(trim "$rest")" DRIFT ;;
        setting) check_setting "$(trim "$rest")" DRIFT ;;
        tool) read -r a b <<< "$rest"; check_tool "$a" "${b:-core}" DRIFT ;;
        cli) check_cli "$(trim "$rest")" ;;
        retire|require)
          a="${rest%%[[:space:]]*}"
          b="$(trim "${rest#"$a"}")"
          check_fragment "$kind" "$pkg" "$a" "$b"
          ;;
      esac
    done < "$f"
  done
}

list_dangling_relations_lines() {
  [ -f "$MACHINERY" ] || return 0
  list_relations > "$TMP/relations"
  cut -d. -f1 "$TMP/relations" | LC_ALL=C sort -u > "$TMP/schemas"
  list_columns | awk '{ split($1, a, "."); print a[1] "." $2 }' | LC_ALL=C sort -u >> "$TMP/relations"
  { grep -noE '`[a-z_]+\.[a-z_]+`' "$MACHINERY" || true; } | tr -d '`' | while IFS=: read -r ln rel; do
    if grep -qxF -- "${rel%%.*}" "$TMP/schemas" && ! grep -qxF -- "$rel" "$TMP/relations"; then
      echo "$ln $rel"
    fi
  done
}

check_machinery() {
  local as_at newest rel ln
  [ -f "$MACHINERY" ] || return 0
  as_at="$(grep -oiE 'as at [0-9]{4}-[0-9]{2}-[0-9]{2}' "$MACHINERY" | head -n1 | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}' || true)"
  newest="$(awk '$1 == "landed" { print $4 }' "$TMP/ledger_all" | grep -E '^[0-9]{4}-' | LC_ALL=C sort | tail -n1 || true)"
  if [ -n "$newest" ] && { [ -z "$as_at" ] || [[ "$as_at" < "$newest" ]]; }; then
    report DRIFT "C7 machinery $MACHINERY as-at date ${as_at:-missing} is older than the newest landed date $newest"
  fi
  list_dangling_relations_lines | while read -r ln rel; do
    echo "$ln $rel"
  done > "$TMP/machinery_dangling"
  while read -r ln rel; do
    report DRIFT "C7 $rel $MACHINERY:$ln names no table or view created in any migration"
  done < "$TMP/machinery_dangling"
}

list_dangling_paths() {
  local doc p bare
  for doc in $OWNED_DOCS; do
    [ -f "$doc" ] || continue
    { grep -noE '`(crates|scripts|frontend|docs)/[^`[:space:]]+`' "$doc" || true; } | tr -d '`' | while IFS=: read -r ln p; do
      case "$p" in *'*'*|*'<'*|*'{'*|*'...'*) continue ;; esac
      bare="$(printf '%s' "$p" | sed -E 's/::.*$//; s/:[0-9]+(-[0-9]+)?$//')"
      [ -e "$bare" ] || echo "$doc $ln $bare"
    done
  done
}

check_paths() {
  local doc ln p
  list_dangling_paths | while read -r doc ln p; do
    in_baseline "path $doc:$p" || echo "$doc $ln $p"
  done > "$TMP/paths_found"
  while read -r doc ln p; do
    report DRIFT "C8 $p $doc:$ln path is not in the tree"
  done < "$TMP/paths_found"
}

undocumented_tables() {
  local t
  while read -r t; do
    grep -qF -- "\`$t\`" "$SCHEMA_DOC" 2>/dev/null || echo "$t"
  done < "$TMP/tables"
}

undocumented_settings() {
  local k
  while read -r k; do
    grep -qF -- "$k" "$FEATURES_DOC" docs/ARCHITECTURE.md docs/CONFIGURATION.md 2>/dev/null || echo "$k"
  done < <(list_settings)
}

undocumented_toggles() {
  local k
  while read -r k; do
    grep -qF -- "\`$k\`" "$FEATURES_DOC" 2>/dev/null || echo "$k"
  done < <(list_toggles)
}

run_all_new() {
  local t k name
  while read -r t; do
    in_baseline "table $t" && continue
    grep -qxF -- "$t" "$TMP/ledger_tables" && continue
    check_table "$t" REVIEW
  done < "$TMP/tables"
  while read -r k; do
    in_baseline "toggle $k" && continue
    grep -qxF -- "$k" "$TMP/ledger_toggles" && continue
    check_toggle "$k" REVIEW
  done < <(list_toggles)
  while read -r k; do
    in_baseline "setting $k" && continue
    grep -qxF -- "$k" "$TMP/ledger_settings" && continue
    check_setting "$k" REVIEW
  done < <(list_settings)
  list_core_tools > "$TMP/core_tools"
  while read -r name; do
    in_baseline "tool $name" && continue
    awk -v n="$name" '$1 == n { f = 1 } END { exit !f }' "$TMP/ledger_tools" && continue
    if grep -qxF -- "$name" "$TMP/core_tools"; then check_tool "$name" core REVIEW; else check_tool "$name" full REVIEW; fi
  done < <(list_tools)
}

run_check() {
  list_tables > "$TMP/tables"
  list_columns > "$TMP/columns"
  process_ledgers
  check_machinery
  check_paths
  if [ "$ALL_NEW" = 1 ]; then run_all_new; fi
  if [ "$FINDINGS" -eq 0 ] && [ "$REVIEWS" -eq 0 ]; then
    echo CLEAN
    return 0
  fi
  if [ "$FINDINGS" -gt 0 ]; then return 1; fi
  if [ "$STRICT" = 1 ]; then return 1; fi
  return 0
}

write_baseline() {
  if [ -f "$BASELINE" ] && [ "$FORCE" = 0 ]; then
    echo "refusing to overwrite $BASELINE without --force" >&2
    return 1
  fi
  list_tables > "$TMP/tables"
  {
    undocumented_tables | sed 's/^/table /'
    undocumented_settings | sed 's/^/setting /'
    undocumented_toggles | sed 's/^/toggle /'
    list_dangling_paths | awk '{ print "path " $1 ":" $3 }'
  } | LC_ALL=C sort -u > "$BASELINE"
  printf 'wrote %s: %s tables, %s settings, %s toggles, %s paths\n' "$BASELINE" \
    "$(grep -c '^table ' "$BASELINE" || true)" "$(grep -c '^setting ' "$BASELINE" || true)" \
    "$(grep -c '^toggle ' "$BASELINE" || true)" "$(grep -c '^path ' "$BASELINE" || true)"
}

self_test() {
  local dir name args out code want got pass=0 fail=0
  local base="$ROOT/$RC/fixtures/judgement-context-docs"
  for dir in "$base"/*/; do
    name="$(basename "$dir")"
    args=""
    [ -f "$dir/args" ] && args="$(cat "$dir/args")"
    set +e
    # shellcheck disable=SC2086
    out="$(bash "$SELF" --root "$dir" $args 2>/dev/null)"
    code=$?
    set -e
    want="$(cat "$dir/expected.txt")"
    got="exit $code${out:+$'\n'$out}"
    if [ "$got" = "$want" ]; then
      echo "ok   $name"
      pass=$((pass + 1))
    else
      echo "FAIL $name"
      printf 'want:\n%s\ngot:\n%s\n' "$want" "$got"
      fail=$((fail + 1))
    fi
  done
  echo "self-test: $pass passed, $fail failed"
  [ "$fail" -eq 0 ] && [ "$pass" -gt 0 ]
}

case "$MODE" in
  selftest) self_test ;;
  baseline) write_baseline ;;
  check) run_check ;;
esac
