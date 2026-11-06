---
title: Perf-evidence — db-perf-check inputs/outputs
status: active
created: 2026-06-16
updated: 2026-07-13
summary: Canonical, tracked home for the before/after EXPLAIN snapshots and HEAD source copies that back a Perf-Verdict / Perf-Exempt commit. Lives next to the graders, NOT in ./artifacts (which is scratch).
---

# Perf-evidence

This folder is the tracked home for the **inputs and outputs of
`scripts/db-perf-check`** — the before/after `EXPLAIN (ANALYZE, BUFFERS)`
snapshots (`*_before.sql` / `*_after.sql`) and any HEAD source copies
(`*_HEAD.rs`) used to diff a query path while measuring it.

It sits beside the DB↔Code graders (`scripts/db-code-graders/`) on purpose:
this evidence backs the `Perf-Verdict:` / `Perf-Exempt:` trailer that the **G6
commit-msg gate** (`g6_perf_evidence.sh`) requires for any SQL-touching commit.
Keeping it here — rather than under `./artifacts/`, which is git-ignored-class
scratch that comes and goes — means the measurement that justified a commit
stays auditable next to the rule that demanded it.

## Why not `./artifacts/`

`./artifacts/YYYY-MM-DD/` is working scratch (phase notes, spikes, review
output). Perf evidence is durable: it is the proof attached to a perf decision
and must survive. The G6 gate also explicitly does **not** count `.sql` here as a
"migration" — these snapshots are db-perf-check's *inputs*, not schema changes,
so they never trigger the perf-evidence requirement on themselves.

## Layout

```
perf-evidence/
  YYYY-MM-DD/
    <label>_before.sql   # query as it was on HEAD, real literal values
    <label>_after.sql    # the new form, real literal values
    <label>_HEAD.rs      # (optional) HEAD source copy for diffing
```

## Workflow

```bash
scripts/db-perf-check --tier <hot|read|periodic|migration> --label <name> \
    --before /tmp/before.sql --after /tmp/after.sql
```

`db-perf-check` copies the before/after queries here automatically
(`<date>/<label>_{before,after}.sql`) and emits a `Perf-Query-SHA:` trailer —
a normalized fingerprint of the measured AFTER query (shared impl:
`scripts/lib/perf-query-sha.sh`). Paste the emitted `Perf-*` block into the
commit message **and `git add` the snapshots it wrote in the same commit**: the
G6 gate requires the staged snapshots, recomputes the fingerprint over the
staged after-snapshot, and (for `.rs` query changes) checks the measured query
against the SQL the commit actually ships (#192). See
`.claude/rules/db-performance-evaluation.md`.
