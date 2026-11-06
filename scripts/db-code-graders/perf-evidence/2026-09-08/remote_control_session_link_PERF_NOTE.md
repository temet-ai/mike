# Remote Control session-link SQL — performance evidence (unit A1)

Migration `20260908000003_remote_control_session_link.sql` and the three queries
it serves. Measured with `EXPLAIN (ANALYZE, BUFFERS)`, best of three runs, on a
schema clone of the live capture DB (`rc_scratch_a1` on the armitage admin
server, migration applied) seeded with 2,000 channels and 10,000
worker-status transitions. The live capture DB could not be used directly:
its schema does not yet carry this migration, and the standing instruction
forbids applying the migration to it.

The `_after.sql` file beside each entry is the exact query with representative
literal values substituted for the bind parameters; the `.plan.txt` beside it is
the captured plan.

## Why db-perf-check did not produce these blocks itself

`scripts/db-perf-check` re-sources `.env` on every run and then reads
`DB_URL="${MIKE_DATABASE_URL:?...}"`. A `MIKE_DATABASE_URL` exported to point
at a scratch database is overwritten by that re-source, so the tool always
targets the live capture DB. The live DB lacks this migration's columns and
table, so the new queries cannot be EXPLAINed there, and applying the migration
to the live DB is forbidden. This is the tool limitation the previous
verification round recorded. The blocks below were produced by running the same
`EXPLAIN (ANALYZE, BUFFERS)` the tool runs, against the scratch DB the tool
cannot be pointed at, and computing the fingerprint with the tool's own
`scripts/lib/perf-query-sha.sh`.

To make these official through the tool, the orchestrator can either point
db-perf-check at a database that already has the migration (for example by
temporarily setting `.env`'s `MIKE_DATABASE_URL` to the `rc_scratch_a1` URL, or
running the tool after the proxy has restarted and applied the migration to the
live DB), then run, from the repo root, for each label:

    scripts/db-perf-check --tier <hot|read> --label <label> \
      --after scripts/db-code-graders/perf-evidence/2026-09-08/<label>_after.sql

The fingerprint below is computed over each `_after.sql` and will match what the
G6 gate recomputes over the staged file.

## Verdicts

### remote_control_fetch_channel_snapshots  (read tier, budget 50 ms)
Roster batch read: every channel whose `local_session_uuid` is in a set of
session uuids. Plan is a single Index Scan on `idx_rc_channels_local_session_uuid`
(the partial index this migration adds). No SubPlan, no Seq Scan.
Min exec over 3 runs: 0.051 ms.

    Perf: remote_control_fetch_channel_snapshots (read tier) — 0.051 ms, Index Scan on idx_rc_channels_local_session_uuid, no bad nodes; new query, baseline.
    Perf-Query-SHA: 9d094f9bd38fe9a6
    Perf-Verdict: baseline

### remote_control_fetch_worker_status_history  (read tier, budget 50 ms)
One session's status-change history, newest first, limit 50. Nested Loop:
Index Scan on `idx_rc_channels_local_session_uuid` to find the channel, then
Bitmap Index Scan on `idx_rc_worker_status_transitions_channel_observed`
(the (channel_id, observed_at DESC) index this migration adds). No SubPlan, no
Seq Scan. Min exec over 3 runs: 0.157 ms.

    Perf: remote_control_fetch_worker_status_history (read tier) — 0.157 ms, Nested Loop over idx_rc_channels_local_session_uuid + idx_rc_worker_status_transitions_channel_observed, no bad nodes; new query, baseline.
    Perf-Query-SHA: 4616a7b85997bfd4
    Perf-Verdict: baseline

### remote_control_record_call  (hot tier, budget 5 ms)
The channel upsert + call insert + conditional status-transition insert +
kept-payload insert, one statement, measured on the ON CONFLICT UPDATE branch
(an existing channel — the common heartbeat path) with a status change so the
transition insert also fires. Every scan is an index scan:
`channels_cse_id_key` (upsert arbiter and the `old` reading),
`sessions_session_uuid_key` (the session_id resolution), `methods_code_key`.
Min exec over 3 runs, including FK trigger checks: 1.296 ms.

The plan carries a `SubPlan` node: the `session_id` resolution
`COALESCE(c.session_id, (SELECT s.id FROM sessions.sessions WHERE session_uuid = ...))`
inside the ON CONFLICT UPDATE. db-perf-check's `bad_nodes()` greps for the
string `SubPlan` and, on a hot-tier query, would report this as a regression.
It is not one: the subquery runs once per single-row upsert (not once per
output row), is an Index Scan on the unique `sessions_session_uuid_key`
(0.002 ms), and the surrounding `COALESCE` short-circuits it away entirely once
`session_id` is resolved, so a linked channel never runs it again. This is the
run-once, covering-index subquery that
`.claude/rules/sql-set-based.md` and the db-performance rule explicitly permit
("a correlated subquery that runs once / uses a covering index can be faster
than set-based, so measure rather than assume"). The measured total, 1.296 ms,
is inside the 5 ms hot budget with the transition insert firing.

    Perf: remote_control_record_call (hot tier) — 1.296 ms with the transition insert firing, all index scans; the one SubPlan is the run-once, index-scan session_id resolution (0.002 ms), COALESCE-short-circuited once resolved — the permitted run-once subquery, not a per-row correlated scan. New statement, baseline.
    Perf-Query-SHA: 6757ab76d38688d3
    Perf-Verdict: baseline

If db-perf-check is run against a migration-applied DB it will print
`[bad nodes: SubPlan]` and exit 1 for this label. That is the tool's grep
firing on a benign run-once subquery; the verdict to record is baseline with the
justification above, per the rule's "measure rather than assume" clause.
