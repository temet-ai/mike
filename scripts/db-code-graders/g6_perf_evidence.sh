#!/usr/bin/env bash
# G6 — Perf-evidence gate
#
# If the change being committed touches SQL (a query!/query_as!/query_scalar!
# macro line, a raw sqlx::query(...) call, or a .sql migration), the commit
# message MUST carry perf evidence that is BOUND to this commit — not just any
# pasted `Perf-Verdict:` line (issue #192). For a passing verdict
# (improved | neutral | baseline), every `Perf-Label:` block must satisfy:
#
#   1. Committed evidence: the measured snapshot is part of the SAME commit as
#      perf-evidence/<date>/<label>_after.sql (plus <label>_before.sql when the
#      block cites Perf-Before-ms). scripts/db-perf-check writes these for you.
#   2. Fingerprint match: the block's `Perf-Query-SHA:` (emitted by
#      db-perf-check) equals the fingerprint recomputed over the committed
#      after-snapshot — the verdict provably describes the committed evidence.
#   2b. Non-degenerate evidence: the after-snapshot must name at least one real
#      table/column identifier. A snapshot with zero meaningful identifiers
#      (`SELECT 1`, `SELECT now()`) cannot be evidence for any shipped query —
#      it fails CLOSED, it does not skip the binding checks.
#   3. Shipped-query match (.rs query changes): the after-snapshot corresponds
#      to SQL this commit actually CHANGES — its normalized text is a substring
#      of the ADDED .rs/.sql diff lines, or ≥ 2/3 of its identifier tokens
#      appear in those added lines. Fallback for queries assembled from const
#      bases + wrappers, and for partial edits of a pre-existing query: ≥ 2/3
#      token overlap with the full changed-file content AND ≥ 1 token overlap
#      with the added lines (the measurement must intersect the actual change).
#      When a migration is co-staged, a block whose snapshot shares identifiers
#      with the migration DDL is accepted as a migration-beneficiary
#      measurement — but at least ONE block must still match the changed .rs
#      query code; a beneficiary measurement never substitutes for measuring
#      the query code the commit itself changes. Pure-migration commits (no
#      .rs query change) skip step 3 entirely: a migration's measured query is
#      often a pre-existing beneficiary of the DDL, legitimately not in the
#      diff (step 2b still applies).
#
# A `Perf-Verdict: regression|over_budget` always blocks. A
# `Perf-Exempt: <reason>` line remains the accepted alternative for SQL changes
# that cannot regress anything.
#
# `Perf-Unavailable: <reason>` records an honest NOT RUN when the capture DB is
# down (mike#163). It is not self-grantable: this gate opens its OWN connection
# to the capture DB and honours the trailer only when that connection actually
# fails. If the DB answers, the trailer is rejected and db-perf-check must run.
#
# Enforces .claude/rules/rust-database-reliability.md at commit time. Called by
# the git commit-msg hook (commit-msg-hook.sh).
#
# Message source (one of):
#   --message-file <path>     (commit-msg hook passes $1; the gate passes an
#                              existing -F/--file path)
#   --message-string <text>   (Claude gate passes the command text — trailers
#                              inside a -m heredoc sit at line start, so the
#                              ^Perf-* greps see them)
#
# Diff scope:
#   --scope cached            (default; commit-msg hook — the index IS final)
#   --scope worktree          (PreToolUse gate on commands that stage AFTER the
#                              hook read the index: `git commit -a/-am/--all`,
#                              `git add … && git commit`. Evaluates the UNION of
#                              the staged diff, tracked working-tree changes vs
#                              HEAD, and untracked files — grade what the
#                              command will actually commit, never exit 0 on a
#                              stale empty index.)
#
# Exit: 0 = allow (no SQL change, bound evidence present, or a probe-verified
#             Perf-Unavailable NOT RUN),
#       1 = BLOCK (SQL changed with missing/unbound/degenerate verdict, a failure verdict,
#           or the fingerprint lib/sha tool being unavailable when checks 2-3 are required).
#
# FAIL-OPEN only for cases that cannot be attacked by tampering: no REPO_ROOT (not in a git
# working tree — no commit is happening) and the fingerprint lib genuinely never existing in
# the repo (a fresh clone predating it). Everything reachable by deleting, breaking, or
# starving a dependency this gate relies on (issue #192) fails CLOSED instead — the caller
# treats ANY non-zero exit as block (see commit-msg-hook.sh), so an internal crash here
# blocks too, rather than being silently interpreted as a pass.
set -uo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0

msg="" scope="cached"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --message-file)   [[ -f "${2:-}" ]] && msg="$(cat "$2")"; shift 2 ;;
    --message-string) msg="${2:-}"; shift 2 ;;
    --scope)          scope="${2:-cached}"; shift 2 ;;
    *) shift ;;
  esac
done
[[ "$scope" == "worktree" ]] || scope="cached"

# ── The effective change being committed ──────────────────────────────────────
# changed_names / added_lines / file_content live in scripts/lib/git-change-scope.sh
# so G3 grades the same commit this gate does (mike#163: two definitions of "the
# change" is how the two gates came to issue contradictory instructions).
# Tracked-but-unloadable fails closed, exactly like perf-query-sha.sh below;
# genuinely absent (a clone predating the lib) still fails open.
GCS_LIB="$REPO_ROOT/scripts/lib/git-change-scope.sh"
if [[ -f "$GCS_LIB" ]] && source "$GCS_LIB" 2>/dev/null; then
  :
elif git -C "$REPO_ROOT" cat-file -e HEAD:scripts/lib/git-change-scope.sh 2>/dev/null; then
  echo "G6 — scripts/lib/git-change-scope.sh is tracked in git but failed to load from the" >&2
  echo "     working tree; the effective change set cannot be computed. Restore it." >&2
  exit 1
else
  exit 0
fi
GCS_ROOT="$REPO_ROOT"
GCS_SCOPE="$scope"

changed_names() { gcs_changed_names "$@"; }
added_lines()   { gcs_added_lines "$@"; }
file_content()  { gcs_file_content "$@"; }

# ── Does the effective diff touch SQL? ────────────────────────────────────────
# Scoped to REAL source changes, so a commit that only carries perf-evidence
# scratch (the inputs/outputs of db-perf-check) — or any other non-source .sql —
# is NOT treated as a schema change requiring its own perf evidence:
#   - migrations are detected by PATH (a `…/migrations/…*.sql` file) only. A
#     `.sql` under artifacts/, scripts/, or docs/ is not a migration. Crucially,
#     the before/after EXPLAIN snapshots db-perf-check consumes (the tracked perf
#     evidence under scripts/db-code-graders/perf-evidence/**/*.sql) ARE the perf
#     evidence — treating them as migrations that themselves demand perf evidence
#     is circular.
#   - artifacts/ is excluded from the .rs query scan: a HEAD snapshot copy of a
#     source file saved for diffing (e.g. perf-evidence/**/foo_HEAD.rs) is
#     evidence, not a source edit.
#
# (1) .sql migration file in the effective change (path-scoped to migrations/)
sql_migration=$(changed_names '*.sql' | LC_ALL=C sort -u | grep -E '(^|/)migrations/' || true)
# (2) added/changed query-macro or raw-query lines in non-test, non-evidence .rs
sql_macro=$(added_lines '*.rs' ':(exclude)*/tests/*' ':(exclude)artifacts/*' ':(exclude,glob)**/perf-evidence/**' \
    | grep -E 'sqlx::query(_as|_scalar)?!?\(|query_as!|query_scalar!|query!\(' \
    | grep -vE '^\+\s*//' || true)

if [[ -z "$sql_migration" && -z "$sql_macro" ]]; then
    exit 0   # no SQL touched — nothing to enforce
fi

# ── SQL changed: require a perf verdict in the message ────────────────────────
if grep -qE '^Perf-Verdict:[[:space:]]*(regression|over_budget)' <<<"$msg"; then
    echo "G6 — perf failure recorded in commit message (Perf-Verdict: regression or over_budget)."
    echo "     A regression (slower than before) or an over-budget query (slower than the tier"
    echo "     target) is rework, not ship. Re-measure with scripts/db-perf-check."
    exit 1
fi

if grep -qE '^Perf-Exempt:' <<<"$msg"; then
    exempt_reason="$(grep -m1 -E '^Perf-Exempt:' <<<"$msg" | sed -E 's/^Perf-Exempt:[[:space:]]*//')"
    if [[ "${#exempt_reason}" -lt 15 ]]; then
      {
        echo "G6 — Perf-Exempt needs a concrete reason, not a placeholder (\"$exempt_reason\")."
        echo "     State truthfully why this SQL change cannot regress anything, e.g."
        echo "     Perf-Exempt: read-only debug script, never runs on a hot/read/periodic path."
      } >&2
      exit 1
    fi
    exit 0   # explicitly, truthfully exempt
fi

# ── Perf-Unavailable: an honest NOT RUN when the DB cannot be measured ────────
# Not self-grantable. The trailer is honoured only when a FRESH connection to the
# capture DB fails here, at commit time — which is what db-perf-check needs and
# cannot get. The probe must open its own backend rather than read through an
# existing pool: a full-disk Postgres keeps serving established connections while
# every new backend dies writing its init file, and that middle stage is what made
# mike#163 a hard deadlock instead of a slow path.
# .env wins over the environment, matching db-perf-check, which sources .env with
# `set -a` AFTER reading the environment. Probing a different URL than the tool
# whose availability is in question would make this verdict meaningless.
capture_db_probe() {   # 0 = reachable, 1 = unreachable, 2 = cannot probe
  local url=""
  if [[ -f "$REPO_ROOT/.env" ]]; then
    url="$(sed -n 's/^[[:space:]]*MIKE_DATABASE_URL=//p' "$REPO_ROOT/.env" | head -1 | sed 's/^"//; s/"$//')"
  fi
  [[ -n "$url" ]] || url="${MIKE_DATABASE_URL:-}"
  [[ -n "$url" ]] || { PROBE_ERR="no MIKE_DATABASE_URL in the environment or $REPO_ROOT/.env"; return 2; }
  command -v psql >/dev/null 2>&1 || { PROBE_ERR="psql is not on PATH"; return 2; }
  PROBE_ERR="$(PGCONNECT_TIMEOUT=5 psql "$url" -X -q -At -c 'SELECT 1' 2>&1)" && return 0
  return 1
}

if grep -qE '^Perf-Unavailable:' <<<"$msg"; then
    unavail_reason="$(grep -m1 -E '^Perf-Unavailable:' <<<"$msg" | sed -E 's/^Perf-Unavailable:[[:space:]]*//')"
    if [[ "${#unavail_reason}" -lt 15 ]]; then
      {
        echo "G6 — Perf-Unavailable needs a concrete reason, not a placeholder (\"$unavail_reason\")."
        echo "     Say what could not be measured and why, e.g."
        echo "     Perf-Unavailable: capture DB on omit refusing new connections (disk full); NOT RUN."
      } >&2
      exit 1
    fi
    PROBE_ERR=""
    capture_db_probe; probe_rc=$?
    case "$probe_rc" in
      1)
        {
          echo "G6 — Perf-Unavailable accepted: the capture DB refused a fresh connection."
          echo "     probe: $PROBE_ERR"
          [[ -n "$sql_migration" ]] && echo "     UNMEASURED migration(s): $(tr '\n' ' ' <<<"$sql_migration")"
          [[ -n "$sql_macro" ]] && echo "     UNMEASURED: query macro/raw-query lines in .rs"
          echo "     The commit records NOT RUN, it does not record a passing verdict. Re-measure"
          echo "     with scripts/db-perf-check once the DB is back and land the evidence."
        } >&2
        exit 0 ;;
      0)
        {
          echo "G6 — Perf-Unavailable rejected: the capture DB answered a fresh connection just now,"
          echo "     so the measurement this trailer claims is impossible is available. Run"
          echo "     scripts/db-perf-check and paste its block, or use Perf-Exempt: <reason> if the"
          echo "     change genuinely cannot regress anything."
        } >&2
        exit 1 ;;
      *)
        {
          echo "G6 — Perf-Unavailable could not be verified ($PROBE_ERR), so it is not honoured."
          echo "     This trailer is only accepted when this gate can watch a connection to the"
          echo "     capture DB fail. Restore the probe path (MIKE_DATABASE_URL in .env, psql on"
          echo "     PATH) and commit again."
        } >&2
        exit 1 ;;
    esac
fi

if ! grep -qE '^Perf-Verdict:[[:space:]]*(improved|neutral|baseline)' <<<"$msg"; then
    # SQL changed but no verdict and no exemption → block.
    {
      echo "G6 — SQL changed but the commit message has no perf evidence."
      [[ -n "$sql_migration" ]] && echo "     migration(s): $(tr '\n' ' ' <<<"$sql_migration")"
      [[ -n "$sql_macro" ]] && echo "     query macro/raw-query lines added or changed in .rs"
      if [[ "$scope" == "worktree" ]]; then
        echo "     (this command stages files at commit time, so ALL tracked + untracked SQL"
        echo "      changes in the working tree count. If the flagged SQL is NOT meant to be"
        echo "      in this commit, stage the intended files explicitly and commit without"
        echo "      -a / -am / git add -A.)"
      fi
      echo ""
      echo "     Measure before/after with real values, then paste the block into the message:"
      echo "       scripts/db-perf-check --tier <hot|read|periodic|migration> --label <name> \\"
      echo "           --before /tmp/before.sql --after /tmp/after.sql"
      echo "     See .claude/rules/rust-database-reliability.md. For a change that cannot"
      echo "     regress anything, add a truthful  Perf-Exempt: <reason>  line instead. If the"
      echo "     capture DB is DOWN and cannot be measured at all, record the honest NOT RUN with"
      echo "     Perf-Unavailable: <what could not be measured and why>  — this gate re-probes the"
      echo "     DB itself and rejects that trailer if the DB is in fact reachable."
    } >&2
    exit 1
fi

# ── Passing verdict: bind the cited evidence to THIS commit (issue #192) ──────
block() {
  {
    echo "G6 — perf evidence is not bound to this commit (issue #192):"
    printf '     %s\n' "$@"
    echo ""
    echo "     Re-run scripts/db-perf-check (it writes perf-evidence/<date>/<label>_{before,after}.sql"
    echo "     and emits Perf-Query-SHA), stage those snapshots WITH the SQL change, and paste its"
    echo "     full block. See .claude/rules/rust-database-reliability.md."
  } >&2
  exit 1
}

# One record per Perf block: label <TAB> query-sha <TAB> has-before-ms
records="$(awk '
  function flush() { if (label != "") print label "\t" sha "\t" hasb; label=""; sha=""; hasb=0 }
  /^Perf-Label:/     { flush(); label=$2 }
  /^Perf-Query-SHA:/ { sha=$2 }
  /^Perf-Before-ms:/ { hasb=1 }
  END { flush() }
' <<<"$msg")"
[[ -n "$records" ]] || block "a passing Perf-Verdict is present but no Perf-Label —" \
    "paste the whole db-perf-check block, not a bare verdict line."

effective_files="$(changed_names | LC_ALL=C sort -u)"

staged_evidence_path() {  # $1 = label, $2 = before|after → prints path, rc 1 if absent
  local f
  while IFS= read -r f; do
    [[ "$f" == *perf-evidence/* ]] || continue
    [[ "${f##*/}" == "$1_$2.sql" ]] && { printf '%s\n' "$f"; return 0; }
  done <<<"$effective_files"
  return 1
}

# Fingerprint + normalization lib (shared with db-perf-check). Tracked-in-HEAD
# but unloadable (e.g. deleted from the working tree without staging the
# deletion) fails closed — checks 2–3 cannot be silently skipped by removing
# the file they depend on. Genuinely absent from the repo (never committed)
# still fails open, so a fresh clone predating the lib isn't bricked.
have_lib=0
if [[ -f "$REPO_ROOT/scripts/lib/perf-query-sha.sh" ]]; then
  # shellcheck source=../lib/perf-query-sha.sh
  source "$REPO_ROOT/scripts/lib/perf-query-sha.sh" 2>/dev/null && have_lib=1
fi
if [[ "$have_lib" != 1 ]] && git -C "$REPO_ROOT" cat-file -e HEAD:scripts/lib/perf-query-sha.sh 2>/dev/null; then
  block "scripts/lib/perf-query-sha.sh is tracked in git but failed to load from the working" \
        "tree — evidence fingerprints cannot be verified. Restore the file (it must not be" \
        "deleted or broken without staging that change) before committing SQL evidence."
fi

# A real measured query names at least one table/column identifier. Zero
# meaningful identifiers (`SELECT 1`) = degenerate evidence = fail closed.
MIN_EVIDENCE_TOKENS=1

# Check 3 applies whenever .rs query code changed — a co-staged migration no
# longer disables it (each block may instead qualify as a migration-beneficiary
# measurement, see the header). Corpora, evidence snapshots excluded (they
# would trivially self-match):
#   added_*  — the ADDED non-migration .rs/.sql diff lines (the query code this
#              commit introduces). Migrations are EXCLUDED here — otherwise a
#              beneficiary query overlapping the DDL identifiers would count as
#              "measures the changed code" and defeat the beneficiary/code split.
#   full_*   — full content of the changed non-migration .rs/.sql files
#              (assembled-query / partial-edit fallback)
#   mig_*    — added lines of co-staged migrations (beneficiary exemption ONLY)
diff_bind=0
[[ -n "$sql_macro" && "$have_lib" == 1 ]] && diff_bind=1
corpus_built=0 added_norm="" added_tokens="" full_tokens="" mig_tokens=""
build_corpus() {
  local f raw_added raw_full=""
  raw_added="$(added_lines '*.rs' '*.sql' ':(exclude)artifacts/*' ':(exclude,glob)**/perf-evidence/**' \
                           ':(exclude)migrations/*.sql' ':(exclude)*/migrations/*.sql' | sed 's/^+//')"
  added_norm="$(perf_sql_normalize <<<"$raw_added")"
  added_tokens="$(tr '[:upper:]' '[:lower:]' <<<"$raw_added" | grep -oE '[a-z_][a-z0-9_]*' | LC_ALL=C sort -u || true)"
  while IFS= read -r f; do
    [[ -z "$f" || "$f" == *perf-evidence/* || "$f" == artifacts/* ]] && continue
    grep -qE '(^|/)migrations/' <<<"$f" && continue
    case "$f" in
      *.rs|*.sql) raw_full+="$(file_content "$f")"$'\n' ;;
    esac
  done <<<"$effective_files"
  full_tokens="$(tr '[:upper:]' '[:lower:]' <<<"$raw_full" | grep -oE '[a-z_][a-z0-9_]*' | LC_ALL=C sort -u || true)"
  mig_tokens="$(added_lines 'migrations/*.sql' '*/migrations/*.sql' | sed 's/^+//' | perf_sql_tokens)"
  corpus_built=1
}

overlap_count() {  # $1 = sorted token list A, $2 = sorted token list B → count of A∩B
  local n
  n=$(LC_ALL=C comm -12 <(printf '%s\n' "$1") <(printf '%s\n' "$2") 2>/dev/null | grep -c . || true)
  printf '%s\n' "${n:-0}"
}

code_covered=0
while IFS=$'\t' read -r label sha hasb; do
  [[ -z "$label" ]] && continue

  # 1 — the measured snapshots are part of this commit
  after_path="$(staged_evidence_path "$label" after)" \
    || block "Perf-Label '$label': its after-snapshot is not part of this commit" \
             "(expected perf-evidence/<date>/${label}_after.sql — the measurement that backs the verdict)."
  if [[ "$hasb" == 1 ]]; then
    staged_evidence_path "$label" before >/dev/null \
      || block "Perf-Label '$label' cites Perf-Before-ms but its before-snapshot is not part of this commit" \
               "(expected perf-evidence/<date>/${label}_before.sql)."
  fi

  [[ "$have_lib" == 1 ]] || continue
  snapshot="$(file_content "$after_path")"

  # 2 — the cited fingerprint matches the committed after-snapshot
  [[ -n "$sha" ]] || block "Perf-Label '$label': no Perf-Query-SHA in its block —" \
      "re-run scripts/db-perf-check (it emits the fingerprint) and paste the full block."
  got_sha="$(perf_query_sha <<<"$snapshot" || true)"
  [[ -n "$got_sha" ]] || block "Perf-Label '$label': could not compute a fingerprint for the" \
      "committed after-snapshot (no working sha256sum/shasum on PATH?) — the cited" \
      "Perf-Query-SHA cannot be verified against it."
  if [[ "$got_sha" != "$sha" ]]; then
    block "Perf-Label '$label': Perf-Query-SHA $sha does not match the committed after-snapshot" \
          "($after_path fingerprints to $got_sha) — the cited measurement is not of the committed evidence."
  fi

  # 2b — degenerate evidence fails closed
  snap_tokens="$(perf_sql_tokens <<<"$snapshot")"
  snap_total=$(grep -c . <<<"$snap_tokens" || true); snap_total="${snap_total:-0}"
  if [[ "$snap_total" -lt "$MIN_EVIDENCE_TOKENS" ]]; then
    block "Perf-Label '$label': the after-snapshot ($after_path) is degenerate — it names no" \
          "table/column identifier at all (e.g. SELECT 1), so it cannot be evidence for any" \
          "shipped query. Measure the query this commit actually ships."
  fi

  # 3 — the measured query corresponds to SQL this commit CHANGES
  if [[ "$diff_bind" == 1 ]]; then
    [[ "$corpus_built" == 1 ]] || build_corpus
    snap_norm="$(perf_sql_normalize <<<"$snapshot")"
    in_added="$(overlap_count "$snap_tokens" "$added_tokens")"
    label_bind=""
    if [[ -n "$snap_norm" && -n "$added_norm" && "$added_norm" == *"$snap_norm"* ]]; then
      label_bind="added-substring"
    elif awk "BEGIN{exit !(${in_added} * 3 >= ${snap_total} * 2)}"; then
      label_bind="added-tokens"
    else
      in_full="$(overlap_count "$snap_tokens" "$full_tokens")"
      if [[ "$in_added" -ge 1 ]] && awk "BEGIN{exit !(${in_full} * 3 >= ${snap_total} * 2)}"; then
        label_bind="changed-file-context"   # assembled base+wrapper / partial edit
      elif [[ -n "$sql_migration" ]]; then
        in_mig="$(overlap_count "$snap_tokens" "$mig_tokens")"
        [[ "$in_mig" -ge 1 ]] && label_bind="migration-beneficiary"
      fi
    fi
    [[ -n "$label_bind" ]] || block \
      "Perf-Label '$label': the measured after-snapshot does not correspond to SQL this commit" \
      "ships (only ${in_added} of ${snap_total} identifiers from $after_path appear in the added" \
      ".rs/.sql lines) — measure the query you are actually changing."
    [[ "$label_bind" != "migration-beneficiary" ]] && code_covered=1
  fi
done <<<"$records"

# When .rs query code changed, at least one evidence block must measure it — a
# co-staged migration's beneficiary measurement never substitutes for measuring
# the query code the commit itself changes.
if [[ "$diff_bind" == 1 && "$code_covered" != 1 ]]; then
  block "the commit changes .rs query code, but every Perf-Label block was accepted only as a" \
        "migration-beneficiary measurement — none measures the changed .rs query itself." \
        "Add a db-perf-check run for the query code this commit changes."
fi

exit 0
