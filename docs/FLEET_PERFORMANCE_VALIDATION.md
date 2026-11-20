---
title: Fleet SQL performance validation
status: superseded
created: 2026-10-13
updated: 2026-11-20
summary: Historical. The real-data EXPLAIN and migration rehearsal procedure written for the G6 gate, which nothing calls any more; it was never run.
---

# Fleet SQL performance validation

**Status: superseded, never run.** This procedure fed the G6 grader
(`scripts/db-code-graders/g6_perf_evidence.sh`), which nothing calls any more, so its
`Perf:` evidence, staged snapshots and trailers gate nothing. It is kept as a record.

## 1. Fix the comparison and budgets before running

Record the proposed commit, actual deployed/prechange revision, PostgreSQL version,
hardware, relevant settings, source snapshot time and selected workload cases. The review
baseline for this tranche was `52e9555f618bbafb677c5ef5d909238434d2322f`; verify whether that
is the real prechange/deployed baseline rather than assuming it is. Save old SQL from the
verified revision and new SQL from the candidate. Compare identical selected records,
cutoffs, array cardinalities and payload sizes on the same copied data and indexes.

| Tier | Absolute statement budget | Before/after tolerance | Noise floor |
|---|---:|---:|---:|
| `hot` | 5 ms | 0% | 0.25 ms |
| `read` | 50 ms | 10% | 2.5 ms |
| `periodic` | 500 ms | 25% | 25 ms |
| `migration` | 5,000 ms | 50% | 250 ms |

Use `--runs 25` for this inventory, including every sub-5-ms pair. The script reports the
best of those runs, not a percentile. Record that aggregation. Do not widen a budget after
a failure. Existing query-specific budgets in the rule take precedence when measuring
those existing surfaces, such as the roster activity join. A regression, new bad SubPlan,
large Seq Scan, or absolute-budget miss requires investigation and remeasurement.

A rewritten query gets a real BEFORE and AFTER pair. A genuinely new C01 query or new
fleet lookup has no old equivalent: run AFTER alone and accept only the tool's measured
`baseline` classification when it meets its budget. Do not invent a `SELECT 1` baseline,
pretend an absent runtime table was a zero-cost query, or call an additive query improved.
For a new statement added to an existing operation, measure the new statement as a baseline
and also compare the complete old/new operation for added round trips and lock duration.

## 2. Prepare an owner-controlled representative scratch copy

Keep connection values in the owner's existing private environment or local untracked
configuration. The commands below refer to `PERF_SOURCE_DB_URL` and `PERF_DB_URL`; neither
value belongs in the repository, SQL evidence, command transcripts or commit messages.
Set `PRIVATE_WORK` to a private directory outside the repository. The scripts also require
`MIKE_DATABASE_URL` to be configured, even when `--db` is supplied; verify the local
configuration and always pass the explicit approved scratch target.

1. Verify and record source and target database identities independently. The scratch
   target must not be the capture database. Use a separate scratch database with the same
   PostgreSQL major version, extensions, collation, deployed schema and indexes.
2. Take one transaction-consistent full source snapshot using the owner's established
   backup/restore procedure. A full database copy is preferred: several queries search
   all history or choose the latest launch, so a recent sample can change the answer and
   plan. Keep dumps, captured bodies and detailed plans private.
3. Record row counts, relation/index sizes and the snapshot timestamp for every table
   below. If using a bounded copy instead of a full clone, copy every row of every table
   read for the complete query time range, plus referenced rows and older latest-launch,
   deduplication and foreign-key history. Document the exact boundary. Queries without a
   time predicate require the full relevant table, not a small sample.
4. Apply only the candidate additive migration to the scratch copy through an explicitly
   approved migration rehearsal. Preserve the live indexes. Do not start a candidate
   server against live simply to obtain the new schema.
5. Populate the new runtime tables from an authorized real observation/identity corpus
   with documented provenance, actual sizes and realistic cardinalities. The existing
   session join targets must be the copied real sessions. An empty runtime schema, unit
   fixtures, randomized synthetic events or generated scale alone cannot satisfy the
   real-data requirement. If no representative runtime corpus is available, mark that
   portion NOT RUN and retain it as a blocker.
6. Run `ANALYZE` on every copied or populated table before measurement. Record table
   counts, runtime payload-size distribution, namespace/thread/event cardinalities,
   mailbox batch sizes and fleet-history depth after preparation.

Minimum dependency inventory; include additional tables touched by the actual restored
constraints/triggers and recursively restore their prerequisites:

| Workload | Copied tables/corpus |
|---|---|
| C01 identity/event queries | `projects.projects`, `sessions.sessions`; representative `runtime.namespaces`, `runtime.threads`, `runtime.events` after migration |
| Mail insertion and closed-recipient audit/routing | `sessions.sessions`, `launch.launches`, `launch.fleets`, `mailbox.messages`, `mailbox.threads`, `mailbox.message_context_items`, `mailbox.failed_sends` |
| Undelivered notices, inactive scan and dead-letter reaper | Previous group plus `sessions.session_goals` and `requests.requests`; retain complete notice-dedup and latest-launch history |
| Acceptance-note author/locking/writes | `workspace.session_notes`, `sessions.sessions`, `launch.fleets`; include downstream carried-lesson/mail tables when timing complete application operations |
| Retrospective checkout/queue scans | Full `launch.fleets` including nested definition JSON and all retrospective phases |
| Closed-mail tripwire | `launch.launches`, `sessions.sessions`, `mailbox.failed_sends`, `launch.told` |
| C02b trusted intake, projection and full-content reads | Full relevant `runtime.namespaces`/`runtime.events` and all seven `runtime.codex_*` tables after the candidate migration; `runtime.threads` plus copied `sessions.sessions`/`projects.projects` when bindings are part of the operation; authorized ordinary-content receipts with authentic original adapter context and real generation/item/effect cardinalities |

Run `ANALYZE table_name;` separately for each listed table that exists in the prepared
schema, including the three C01 and seven C02b runtime tables. Do not discard slow fleets, large notes,
long launch histories or large event payloads from the chosen cases. Include ordinary,
high-cardinality and negative/duplicate cases; document cases unavailable in real data.

## 3. Exact C01 query inventory

These are the seven proposed statements from
`crates/mike/src/store/persistence/runtime/{identity,events}.rs`. Each SQL block is a
parameter template, not a directly runnable measurement file. Substitute the actual
selected values with correct SQL quoting/types and save each resolved statement as its
own private `.sql` file. Never leave `$1` parameters unresolved or use arbitrary `1`/`-1`
IDs. Recheck the source at the commit being graded before copying.

Use `hot`, 5 ms for inserts and their replay-lookup statements; use `read`, 50 ms for
standalone lookups. The more conservative hot budget covers the replay path even where
the same lookup also serves a non-hot inspection call. Compare complete replay-operation
latency separately because it executes an insert attempt and a read.

### `c01_namespace_insert` — register namespace

```sql
INSERT INTO runtime.namespaces (id, runtime_kind, durable_key, identity)
VALUES ($1, $2, $3, $4)
ON CONFLICT DO NOTHING RETURNING id;
```

Bindings: namespace UUID, runtime text, durable key, full validated identity JSONB.
Measure a new key, exact retry, same-ID conflict and same-scope/different-ID conflict.

### `c01_namespace_lookup` — fetch namespace

```sql
SELECT identity FROM runtime.namespaces WHERE id = $1;
```

Binding: real namespace UUID. Measure present and genuinely absent lookup cases; an
absent case supplements a populated-table case and is never the sole evidence.

### `c01_thread_bind` — bind Mike session to provider thread

```sql
INSERT INTO runtime.threads
    (session_id, namespace_id, provider_thread_id, provider_session_id, owning_seat_id,
     native_parent_namespace_id, native_parent_thread_id, fork_namespace_id,
     fork_thread_id, identity)
SELECT session.id, $2, $3, $4, owner.id, $6, $7, $8, $9, $10
FROM sessions.sessions session
LEFT JOIN sessions.sessions owner ON owner.session_uuid = $5
WHERE session.session_uuid = $1 AND ($5::uuid IS NULL OR owner.id IS NOT NULL)
ON CONFLICT DO NOTHING RETURNING session_id;
```

Bindings: Mike session UUID; namespace UUID; opaque thread ID; optional shared provider
session ID; optional owning-seat UUID; native-parent namespace and thread; fork namespace
and thread; full binding JSONB. Cover ownerless seats, native children, unresolved
parent/fork references, identical replay, conflicting binding and missing-session/owner.
Use real copied sessions; do not fabricate production identities for a timing shortcut.

### `c01_thread_lookup` — fetch by scoped provider identity

```sql
SELECT identity FROM runtime.threads
WHERE namespace_id = $1 AND provider_thread_id = $2;
```

Bindings: namespace UUID and opaque provider thread ID. Include the same provider thread
string in distinct real namespaces where available, plus a miss at realistic table size.

### `c01_thread_session_lookup` — fetch by Mike UUID

```sql
SELECT thread.identity FROM runtime.threads thread
JOIN sessions.sessions session ON session.id = thread.session_id
WHERE session.session_uuid = $1;
```

Binding: Mike UUID. Verify indexed resolution on both session and binding at actual scale.

### `c01_event_append` — append raw observation

```sql
INSERT INTO runtime.events
    (namespace_id, ingestion_id, provider_thread_id, envelope, raw_payload, parsed_payload)
VALUES ($1, $2, $3, $4::text::json, $5, $6::text::json)
ON CONFLICT (namespace_id, ingestion_id) DO NOTHING RETURNING id;
```

Bindings: namespace UUID, durable ingestion UUID, optional provider thread ID, serialized
envelope text, original BYTEA payload, optional serialized projection text. Preserve the
TEXT-to-JSON casts: measuring JSONB instead misses the actual NUL-safe write path.
Cover normal and large real payloads, malformed/raw-only observations where available,
exact replay, changed-content conflict and repeated provider-item updates with distinct
ingestion IDs. Seed replay rows before measurement; keep replay bytes and receive time
identical. For a first-insert case use a real recorded observation not yet imported into
that scratch table, and let rollback restore the case between runs.

### `c01_event_lookup` — read exact stored observation

```sql
SELECT envelope, raw_payload, parsed_payload FROM runtime.events
WHERE namespace_id = $1 AND ingestion_id = $2;
```

Bindings: namespace and ingestion UUIDs. Include large/TOASTed payloads and replay reads.
`EXPLAIN ANALYZE` does not measure full client transfer/deserialization or Rust's byte and
metadata comparison, so also time the actual full-result read and replay operation on
scratch. Do not project away the raw payload to make that application measurement fast.

## 4. Other fleet repair query groups

Paths below are relative to `crates/mike/src/store/persistence/`. Capture all statements
in each group, including newly executed statements whose SQL text did not change.

| Group / source | Comparison and cases | Tier / budget |
|---|---|---|
| `mailbox/insert_threaded_message.sql`; old inserts in `delivery.rs` and `thread_send.rs` | Old unguarded insert versus new latest-launch guard; open, closed and relaunched recipients; no context versus context transaction; direct and threaded sends | `hot` / 5 ms per statement; compare complete old/new send operation |
| Session-ID lookups and context inserts in `mailbox/thread_send.rs` | Record extra round trips introduced by unified routing; empty, typical and large real context batches | `hot` / 5 ms |
| `mailbox/closed_rejection.rs::audit_closed_recipient` | Additive audit insert baseline; closed existing recipient and missing/absent author path | `hot` / 5 ms |
| `mailbox/fleet_seat.rs`, `fleet_target.rs` and their existing lookup callees | Latest launch, fleet resolution and open successor/coordinator checks are invoked on more paths; measure actual old/new query count and cumulative cost | `read` / 50 ms per lookup, plus operation latency |
| `mailbox/undelivered_notice.sql` versus old `undelivered.rs::tell_sender_not_delivered` SQL | Single and batch notices; existing notice, consumed mail, missing sender, open/closed fleet seat, multi-task definition and deep launch history | `periodic` / 500 ms; compare single-message pair and complete batch work |
| `mailbox/undelivered.rs::tell_inactive_senders` | Additive inactive-message selection plus notice call; typical and large real queues; recent activity and already-notified exclusions | `periodic` / 500 ms |
| `mailbox/redelivery.rs::dead_letter_expired_undelivered` | Old single CTE versus new reaping/audit CTE plus notice statement in one transaction; zero, typical and largest observed expired batches | `periodic` / 500 ms per statement, plus full transaction latency |
| `session_notes/reads.rs::session_note_belongs_to` | Additive ownership-existence lookup; matching, mismatching and absent IDs | `read` / 50 ms |
| `session_notes/obligation.rs` | New fleet definition `FOR SHARE`, author join and note `FOR UPDATE` lookups; real large definitions/notes and concurrent readers/writers | `read` / 50 ms per statement; record lock waits separately |
| `session_notes/writes.rs` create/update/delete | Existing statements now execute with validation/locking inside transactions; compare actual old/new operation, ordinary notes and obligation/ruling notes, protected rejection paths | `read` / 50 ms per statement, plus transaction/lock duration |
| `fleet/fleet_checkout_users.sql` | Additive scan baseline; active fleet states plus done fleets with pending/running/blocked retrospective definitions | `read` / 50 ms |
| `fleet/pending_retrospectives.sql` | Additive full-history queue scan baseline; populated and empty result sets at real history size | `periodic` / 500 ms |
| `fleet/closed_mail_attempts.sql` | Additive tripwire baseline; real run/time window, repeated launches, already-told exclusion and largest observed failed-send volume | `periodic` / 500 ms |

For statements moved from inline Rust into `.sql`, the BEFORE is the original inline SQL,
not an empty file. The C01 reader audit in [RUNTIME_CONTRACT.md](RUNTIME_CONTRACT.md) lists
future migration work; those untouched queries are not falsely counted as changed here.
Reconcile this inventory with the final diff, including untracked/new `.sql` files, so
later additions do not escape measurement merely because they were absent from this list.

## 5. Run one resolved statement per measurement

Choose real records safe to include in committed SQL evidence. Do not commit credentials,
authentication material, private connection URLs or captured content lacking approval.
If no safe representative record exists, stop and arrange approved evidence handling.
Changing or redacting a measured snapshot afterward invalidates the binding: rerun the
exact safe snapshot that will be committed. Keep private dumps and complete logs outside
the repository.

The owner supplies the existing private environment values and resolved SQL files first.
These commands are examples of the actual runner interface, not a report that it ran:

```sh
: "${PERF_DB_URL:?Set the approved scratch database privately}"
: "${PRIVATE_WORK:?Set a private directory outside the repository}"

bash scripts/db-perf-check --db "$PERF_DB_URL" \
  --tier hot --budget 5 --runs 25 --label fleet_mail_insert_open \
  --before "$PRIVATE_WORK/fleet_mail_insert_open_before.sql" \
  --after "$PRIVATE_WORK/fleet_mail_insert_open_after.sql" \
  > "$PRIVATE_WORK/fleet_mail_insert_open.log" 2>&1
result=$?
cat "$PRIVATE_WORK/fleet_mail_insert_open.log"
test "$result" -eq 0

bash scripts/db-perf-check --db "$PERF_DB_URL" \
  --tier hot --budget 5 --runs 25 --label c01_event_append_new \
  --after "$PRIVATE_WORK/c01_event_append_new_after.sql" \
  > "$PRIVATE_WORK/c01_event_append_new.log" 2>&1
result=$?
cat "$PRIVATE_WORK/c01_event_append_new.log"
test "$result" -eq 0
```

Run and inspect each command independently; do not let a later successful command hide a
failure. Never pipe the runner into `tail` or grade it from a wrapper's exit status. Save
the full EXPLAIN plans/buffers as separate owner-private logs when needed: the runner's
printed block contains timing/verdict, not a complete archival plan. Obtain those plans
with the same exact resolved SQL and `EXPLAIN (ANALYZE, BUFFERS)`.

The runner wraps detected INSERT/UPDATE/DELETE statements in `BEGIN`/`ROLLBACK`. It does not
wrap a locking SELECT solely because it says `FOR UPDATE` or `FOR SHARE`. Use the scratch
copy for locking/mutation workloads. Rollback does not undo sequence advancement or every
possible nontransactional trigger side effect; review triggers and do not describe this
as permission to execute arbitrary writes on live.

The runner accepts **one EXPLAIN-able statement per file**. Measure multi-statement reaper,
mail-context and note-validation operations statement by statement, then separately record
the actual complete transaction/application latency against the same cases. For a manual
transaction rehearsal, use `BEGIN`, each exact `EXPLAIN (ANALYZE, BUFFERS)` in execution
order, and `ROLLBACK`; preserve realistic data dependencies between statements. Do not
compare only one new statement with a whole old operation or claim that summing server
execution times captures round-trip or lock costs.

## 6. Rehearse the migration separately

The migration is `20261013130000_runtime_identity_events.sql`. It creates a schema, three
tables, constraints and indexes; it has no pre-existing C01 DDL equivalent. PostgreSQL
cannot `EXPLAIN CREATE TABLE` or `EXPLAIN CREATE INDEX`, so do not feed this multi-statement
file to `db-perf-check` or manufacture a plan for it.

On a disposable clone at the real prechange ledger, record the exact migration checksum,
wall-clock duration, lock waits, relation/index sizes and errors while applying it through
the supported migration workflow. Use the 5-second migration budget as the initial absolute
rehearsal target. Test representative concurrent activity separately on scratch because
foreign-key creation touches existing session tables. Repeat from a fresh equivalent
snapshot when rerunning; an already-created schema is not a valid new-apply timing case.
This does not authorize applying it to capture or treating manual SQL application as an
update to the production migration ledger.

Measure the resulting new write/read beneficiaries from section 3 against the populated,
ANALYZEd post-migration copy. Record those exact query snapshots and fingerprints. The DDL
rehearsal supplements them; it cannot replace changed Rust-query evidence. If a later
migration changes a pre-existing query's plan, compare that real query before and after
on equivalent pre/post-migration copies and retain both measurements.

## 7. Bind evidence and retain the post-deploy obligation

For each case, keep the tool's emitted `Perf-Tier`, `Perf-Label`, optional `Perf-Before-ms`,
`Perf-After-ms`, `Perf-Budget-ms`, `Perf-Verdict` and `Perf-Query-SHA` exactly as measured.
Do not prefill numeric values or a passing verdict from this document. Add the scratch
identifier (a safe database name, not a URL), source snapshot time, per-table row counts,
corpus provenance and size distribution, selected case, tier budget and runs count to the
`Perf:` evidence narrative.

The runner writes resolved snapshots to
`scripts/db-code-graders/perf-evidence/<date>/<label>_after.sql` and, for real pairs,
`<label>_before.sql`. Stage the precise measured snapshots in the same commit as the SQL
change, after checking them for secrets. G6 must match the normalized AFTER fingerprint
and bind that statement to the changed Rust/SQL. Do not edit snapshots after measurement,
substitute a migration-only beneficiary for all changed Rust queries, or use an unrelated
fast query to obtain a green gate. A one-query minimum in the automated gate is not a
waiver of the full inventory.

Name every scratch-measured query on the commit's **post-deploy live remeasurement list**.
After an independently authorized deployment, repeat the representative measurements on
live through the owner-approved read/rolled-back-write procedure, preserving budgets and
cases, and record actual versions/data sizes. Review mutation triggers and sequence
side effects before approving live write measurements. Keep unsupported Codex launch
closed and do not promote a newly enabled runtime ingest path beyond shadow until its
real live measurements are within budget. Regressions or budget misses block promotion.

If the real capture database is unreachable, only a runner connection failure and G6's
independent probe can support `Perf-Unavailable`; that still means NOT RUN, never passed.
An unavailable client binary, absent local configuration, missing authorization, query
error, unavailable real runtime corpus or an unexecuted procedure does not establish
that the capture database is down. Report the actual blocker and keep validation open.

## 8. C02b trusted receipt and projection storage

**Status: NOT RUN.** C02b adds public Rust persistence methods but no external route,
transport, history fetcher or live-agent acceptance path. Its local authored tests are
not real-data performance evidence. The owner's draft-only publication exception leaves
this validation outstanding before merge; it does not produce a passing G6 verdict.

Correctness fixtures comprise 44 tests in `crates/mike/tests/integration/runtime_codex/`,
registered in the existing integration harness: 19 codec/validation cases and 25 database
cases. All 44 passed on 2026-10-13 with disposable PostgreSQL configured, with no ignored
cases. All 233 assertions were preserved; three private-validator call routes changed
mechanically to feature-only delegates. Final strict workspace/all-target Clippy passed.
Repeated monolithic library-harness builds were killed near 7.7 GiB RSS; full-workspace
executed-test validation remains outstanding. No test was disabled to obtain these results.
The DB fixtures still require the isolated ephemeral database helper to supply a database;
early returns without it are not executed DB assertions. Neither source inventory,
compilation, Clippy nor fixture results can replace the real-data measurements below.

### 8.1 Real-data preparation and comparison

Extend section 2's approved representative scratch copy with all seven tables:
`runtime.codex_intake_state`, `runtime.codex_receipt_context`,
`runtime.codex_projection_generations`, `runtime.codex_projection_heads`,
`runtime.codex_projection_applications`, `runtime.codex_projection_effects` and
`runtime.codex_item_state`. Include full relevant C01 namespace/event history and all
FK prerequisites. Queries without a time bound, especially promotion completeness and
unclassified raw scans, require the full relevant tables. Record counts/sizes for each,
including pre-existing `runtime.events`, and preserve indexes and constraints.

Use authorized ordinary-content observations with their independently established original
adapter context. Record corpus source time, namespace/epoch/thread/turn/item counts,
context/projection versions, effect fanout, payload/state sizes, generation count, applied
prefix depth and legacy-unclassified proportion. Do not reclassify existing legacy receipts
by guessing trust, attach a fabricated context to captured responses, or include excluded
authentication content. Do not run a new live transport or launch agents to fill a benchmark.
If no approved representative corpus with legitimate context exists, the affected cases
remain NOT RUN. Unit fixtures and generated volume may supplement correctness/stress
exploration but cannot satisfy this real-data requirement.

`ANALYZE` every copied/populated table, including `runtime.events` and all seven additions.
Preserve ordinary and large/TOASTed documents, deep generation histories, repeated same-item
updates, high effect fanout and unknown/malformed admitted frames where the real corpus
contains them. Record unavailable cases instead of manufacturing production provenance.

Record the actual candidate/prechange revisions. Genuinely new C02b statements have no
old equivalent: use AFTER-only baseline measurements, with the verdict emitted by the
runner. Never invent a zero-cost pre-C02b projection, empty-table control or `SELECT 1`
BEFORE. For existing C01 raw writes/reads affected by the new event index or an actual
source change, compare their real old/new SQL on equivalent pre/post-migration copies.
If characterizing trusted-intake overhead relative to C01 raw append, explicitly label the
extra context/order contract and measure both complete operations; that comparison is not
proof that the two operations provide equivalent guarantees.

Pin absolute budgets before running: `hot`/5 ms for intake and per-receipt projection SQL;
`read`/50 ms for standalone inspection and generation operations; `periodic`/500 ms for
the full-namespace promotion completeness scan; `migration`/5,000 ms for the DDL rehearsal.
Use 25 runs and the existing tier tolerances/noise floors. A query used both on a hot path
and standalone must meet its hot budget. Do not widen these after a failure. Additionally
predeclare complete-operation budgets and record actual SQL round trips, client decoding,
transaction/commit duration and lock waits; summing EXPLAIN execution times is insufficient.

### 8.2 Production SQL inventory

Every source path in this table is relative to
`crates/mike/src/store/persistence/runtime/codex/`. Locate each statement by file and
symbol, and copy its complete literal SQL from the final integrated commit into one
resolved SQL file per measurement. The query shapes below identify the statement within
multi-query functions; they are not executable measurement substitutes. Labels are proposed
case-label stems, not emitted results. Test-only SQL is not a production workload; test
success never substitutes for these measurements.

| Label stem | Exact source / SQL shape | Cases / tier |
|---|---|---|
| `c02b_intake_state_insert` | `intake.rs::lock_intake`; INSERT intake state SELECT registered namespace WHERE `runtime_kind = 'codex'` ON CONFLICT DO NOTHING | First use and existing state, missing/wrong-runtime namespace; hot |
| `c02b_intake_state_lock` | `intake.rs::lock_intake`; SELECT `last_capture_sequence` FROM intake state WHERE namespace FOR UPDATE | Uncontended and concurrent same/different namespaces; hot, lock waits separate |
| `c02b_receipt_lookup` | `intake.rs::existing_receipt` and `reads.rs::runtime_receipt`; SELECT complete event/context documents and raw bytes, LEFT JOIN context on event ID and namespace, WHERE namespace/ingestion | New miss, exact retry, changed-context conflict, legacy row, full large payload; hot because intake reuses it |
| `c02b_transport_latest` | `intake.rs::transport_order`; SELECT ordinal by namespace/epoch ORDER BY ordinal DESC LIMIT 1 | First epoch receipt, deep epoch, exact-next/skip/regression; hot |
| `c02b_raw_insert` | `intake.rs::insert_observation`; INSERT events with text-to-JSON envelope/projection, ON CONFLICT DO NOTHING RETURNING id | New ordinary/large receipt and concurrent legacy conflict; hot; C01 event-write index before/after comparison also required |
| `c02b_capture_counter` | `intake.rs::insert_context`; UPDATE intake counter + 1 WHERE namespace RETURNING sequence | Already locked real namespace; hot |
| `c02b_context_insert` | `intake.rs::insert_context`; INSERT complete context with `::text::json` | New real context, typed RPC ID/scope, constraint conflict; hot |
| `c02b_raw_page` | `reads.rs::raw_event_page`; SELECT complete event/context documents and raw bytes, LEFT JOIN context, namespace/id keyset, `NOT $3` with false unclassified filter, ORDER BY event ID LIMIT | First/middle/end pages, mixed history, limits 1/typical/1000, full payload; read |
| `c02b_unclassified_page` | `reads.rs::raw_event_page`; same full receipt SELECT with true unclassified filter requiring `context.event_id IS NULL`, namespace/id keyset, ORDER BY event ID LIMIT | First/middle/end pages, mixed and legacy-heavy history, limits 1/typical/1000, full payload; read |
| `c02b_generation_insert` | `generation.rs::begin_codex_projection_generation`; INSERT version-pinned generation with state `'building'` ON CONFLICT DO NOTHING | New generation and same-ID retry; read, plus shared intake locks |
| `c02b_head_insert` | `generation.rs::begin_codex_projection_generation`; INSERT namespace head ON CONFLICT DO NOTHING | First generation and existing namespace head; read |
| `c02b_generation_lookup` | `generation.rs::codex_projection_generation`; SELECT generation ID, namespace, versions, state and checkpoint WHERE generation ID | Present/missing and many historical generations; read; include extra lookup executed before each projection-data read |
| `c02b_active_generation_lookup` | `generation.rs::active_codex_projection_generation`; SELECT generation metadata from head/generation join on ID and namespace | No active head and active generation; read |
| `c02b_generation_lock` | `generation.rs::lock_generation`; SELECT generation ID, namespace, versions, state and checkpoint WHERE generation ID FOR UPDATE | Begin/apply/promote, concurrent apply workers; hot for application, lock waits separate |
| `c02b_head_lock` | `generation.rs::activate_codex_projection_generation`; SELECT active generation FROM heads WHERE namespace FOR UPDATE | First promotion, replacement, already-active retry; read |
| `c02b_generation_complete` | `generation.rs::check_complete`; COUNT context LEFT JOIN applications by generation/event WHERE namespace and unapplied | Small/full namespace, many generations, one gap, fully caught up; periodic, inside intake-lock boundary |
| `c02b_generation_retire` | `generation.rs::promote`; UPDATE previous generation SET state = 'retired' WHERE generation ID | Existing old active ID and first-promotion NULL; read |
| `c02b_generation_activate` | `generation.rs::promote`; UPDATE candidate SET state = 'active', activated_at = now() WHERE generation ID | Caught-up building candidate; read |
| `c02b_head_switch` | `generation.rs::promote`; UPDATE heads SET active generation WHERE namespace | First and replacement head; read |
| `c02b_next_receipt` | `apply.rs::next_receipt`; SELECT complete event/context documents and raw bytes, LEFT JOIN context, namespace and sequence above checkpoint, NOT EXISTS application for generation/event, ORDER BY sequence LIMIT 1 | Beginning/deep prefix, caught-up miss, large receipt, many historical generations; hot |
| `c02b_effects_insert` | `effects.rs::insert_effects`; one INSERT SELECT from six UNNEST arrays, document text cast to JSON | Zero/typical/largest real effect batch, all five kinds, unknown/NUL-bearing provider data where available; hot |
| `c02b_item_states_load` | `state.rs::load_states`; SELECT DISTINCT complete item states including `state_document::text`, JOIN three UNNEST key arrays WHERE generation | New/existing/mixed items, duplicate keys in one batch, large full state; hot |
| `c02b_item_states_upsert` | `state.rs::save_states`; INSERT SELECT UNNEST batch ON CONFLICT key DO UPDATE flags/latest sequence/full document | New, delta update, terminal late-update and history-enrichment states; hot |
| `c02b_application_insert` | `apply.rs::record_application`; INSERT application status/issues/effect count with `::text::json` | Projected/partial/unsupported/malformed applications, zero/large issue arrays; hot |
| `c02b_generation_checkpoint` | `apply.rs::record_application`; UPDATE generation SET last applied sequence WHERE generation ID | Real generation after complete receipt effects/state; hot |
| `c02b_item_lookup` | `projection_reads.rs::codex_projected_item`; SELECT complete state including `state_document::text` WHERE generation/namespace/thread/turn/item | Existing/missing item, full large snapshot and separate stream text; read |
| `c02b_item_page` | `projection_reads.rs::list_codex_items`; SELECT complete states including `state_document::text`, generation/thread and tuple keyset `(first_capture_sequence, turn_id, item_id)`, ORDER BY matching tuple LIMIT | First/deep/end page, ties, many updates, limits 1/typical/1000; read |
| `c02b_effect_page` | `projection_reads.rs::list_codex_projection_effects`; SELECT event/sequence/ordinal and full `observation::text`, optional kind and tuple keyset `(capture_sequence, effect_ordinal)`, ORDER BY matching tuple LIMIT | All kinds and each kind, high fanout, first/deep/end pages, full observations; read |
| `c02b_application_lookup` | `projection_reads.rs::codex_projection_application`; SELECT application metadata and full `issues::text` WHERE generation/event | Present/missing, large issues, unsupported/partial receipt; read |
| `c02b_issue_page` | `projection_reads.rs::list_codex_projection_issues`; SELECT applications with full `issues::text`, generation/sequence keyset plus `json_array_length(issues) > 0`, ORDER BY sequence LIMIT | Sparse/dense issues across deep history, zero-result scan, full arrays; read |

`list_codex_usage_observations` and `list_codex_lifecycle_observations` call the same effect
page SQL with a forced kind. Measure those selectivities; do not count them as different
SQL implementations. `raw_event_page` likewise needs both Boolean filter settings.
All projection-data reads first call the generation lookup for version validation; include
that round trip in full-operation measurements. Generation create/promote also use the
shared intake-state insert/lock, and create/apply/promote reuse the generation lock.

Keep exact BYTEA payloads and full JSON documents in reads and writes. Preserve the
source's text-to-JSON casts and aligned array cardinalities; a JSONB surrogate, metadata-only
SELECT or smaller text payload is a different measurement. Unknown user/provider data must
not enter committed snapshots without permission. Reuse section 5's private evidence
handling and safe-record selection, and rerun if a measured snapshot must change.

### 8.3 Complete-operation and concurrency measurements

Use statement-level EXPLAINs plus actual public-method timings on the approved scratch
database. Record each method's whole input class and full returned data size:

- Capture: first namespace receipt, ordinary established intake, exact retry, conflicting
  retry, same-namespace contention and independent namespaces. Count admission/serialization
  time, namespace-lock wait and all round trips. Excluded frames return before database
  access and are a separate correctness case, not an SQL baseline.
- Application: zero-effect receipt, one item, repeated keys/multiple effects, large history
  page and deep replay prefix. Include generation lock, full raw/context decode, batch effect
  insert, full state load/reduction/upsert, application marker, checkpoint and commit.
- Replay: empty new generation through a real recorded corpus, with unchanged raw/context
  evidence. Report receipts/effects/items per second, payload sizes and per-call distribution;
  bounded per-statement performance alone does not establish whole-replay throughput.
- Promotion: first activation, replacement, incomplete rejection, repeated activation and
  concurrent intake. Measure the full missing-application scan and the time it holds the
  intake lock. A large history scan may block capture even if each row lookup is indexed.
- Inspection: complete raw/item/effect/issue pages at the same cursor, selectivity and row
  limit. Include client transfer/decoding and the preliminary generation check, not just
  server execution. Pin a generation when comparing pages; active item contents may change.

Preserve the transaction's real dependencies: capture inserts raw/context together, and
projection writes effects/state before the application marker. The deferred effect-to-
application FK is checked at commit, so a rollback-only statement rehearsal omits that
commit cost. On the disposable scratch target, measure actual successful complete method
commits and restore the controlled case outside the timed interval. If manually checking a
rollback rehearsal's integrity, issue `SET CONSTRAINTS ALL IMMEDIATE` after all dependent
rows exist and before rollback; that check supplements rather than replaces real commit
timing. Never run application writes on live merely because individual EXPLAINs roll back.

Exercise worker contention with predetermined inputs and bounded concurrency on scratch;
record wait time, transaction count and output equivalence. Capture sequence is committed
intake order, not proof of transport completeness. `transport_gap = false`, `CaughtUp`,
unit-test success and an empty issue page are not substitutes for real-data evidence.

### 8.4 Additive DDL rehearsal and post-deploy obligation

The candidate C02b migration is `20261014070000_codex_projection_receipts.sql`.
Record its exact integrated checksum before rehearsal. It creates seven tables,
their primary/unique/FK/check constraints and indexes, plus unique B-tree
`idx_runtime_events_namespace_id` on existing `(namespace_id, id)`. That event index is a
new write/storage cost even though its identifier pair is already logically unique.
This is not inert DDL and has no earlier C02b DDL equivalent.

Rehearse on a fresh equivalent pre-C02b scratch snapshot using section 6's supported,
explicitly approved migration workflow. Record wall time, index/relation sizes, lock waits,
errors and impact under representative concurrent activity. The ordinary CREATE INDEX
on a populated `runtime.events` table needs particular lock/build-time scrutiny. Do not
feed the multi-statement DDL to EXPLAIN or pretend an already-created schema is a first-
apply measurement. Start with the 5-second migration budget; investigate overruns.

Measure real existing C01 raw insert/read beneficiaries before and after the event index,
as well as every applicable new Rust SQL group above on the populated post-migration copy.
DDL timing cannot replace code-bound changed-query evidence. Keep actual snapshots,
fingerprints and emitted verdicts in the same commit as the measured SQL changes under
section 7's rules. Until measurements exist, report NOT RUN and its actual blocker; do not
invent `Perf-Verdict`, `Perf-Exempt` or `Perf-Unavailable` trailers.

Add all scratch-measured C02b groups and the affected C01 operations to the post-deploy live
remeasurement list. Deployment and live writes require their own authorization, with trigger,
sequence and lock side effects reviewed. Keep new runtime ingest below promotion beyond
shadow until its live measurements meet budget. This procedure authorizes neither deployment
nor live transport/agent/account use; missing corpus or access remains an explicit blocker.


## 9. Fleet stopping and lifecycle admission (#588)

Correctness evidence: the existing integration harness passed 34 lifecycle cases against
an isolated PostgreSQL instance on 2026-10-13. They cover stale startup, transaction rollback,
launch SHARE/stop ordering, all unresolved rows, pending command namespace boundaries,
retrospective staging and monotonic closure evidence. They do not establish query performance
or validate the still-in-progress managed-process and shutdown integration.

**G6 representative-data performance evidence: NOT RUN.** Apply sections 6–7 to these
literal queries in `crates/mike/src/store/persistence/fleet/` unless otherwise named.
Use real identifiers and representative maximum run/history sizes from the owner's approved
scratch dataset. No live-data access or deployment is implied by this procedure.

| Label | Tier/budget | Actual added/moved source | Before/comparator |
|---|---|---|---|
| fleet_lifecycle_lock | read / 50ms | lifecycle_sql/lock_fleet.sql; lifecycle.rs::lock_fleet | find_fleet_by_uuid.sql (same exact UUID lookup without lock); measure lock-wait behavior separately |
| fleet_guarded_state_write | read / 50ms | lifecycle.rs::write_state; existing set_fleet_state.sql macro | unchanged set_fleet_state.sql; compare complete transaction latency with lock acquisition separately |
| fleet_unsettled_launches | read / 50ms | lifecycle_sql/unsettled_launches.sql; lifecycle_rows.rs | new baseline for complete exact-run unresolved roots/reservations; old list_launches.sql LIMIT 200 is not semantically equivalent |
| fleet_pending_commands | read / 50ms | lifecycle_sql/pending_commands.sql; lifecycle_rows.rs | new baseline for exact run or literal run# namespace over unfinished receipts |
| fleet_run_settled | read / 50ms | lifecycle_sql/run_settled.sql; lifecycle_completion.rs | new baseline; covers both completion and failed-retry checks |
| fleet_run_admission | read / 50ms | lifecycle_sql/run_admission.sql; lifecycle_rows.rs | exact-run fleet lookup baseline; index idx_launch_fleets_run_id already exists |
| fleet_launch_admission_lock | read / 50ms | lifecycle_sql/lock_run_admission.sql; lifecycle_launch.rs | run_admission query before; measure backend-open lock hold separately, not as EXPLAIN execution time |
| fleet_launch_admission_owner | read / 50ms | lifecycle_launch.rs::record_opened literal SELECT id FROM launch.launches WHERE id=$1 AND run_id=$2 FOR UPDATE | new exact-id/run baseline using existing launch primary key |
| fleet_launch_open_record | read / 50ms | lifecycle_launch.rs existing record_launch_opened.sql macro | unchanged record_launch_opened.sql; same-transaction holding behavior is separate concurrency evidence |
| fleet_closure_route_lookup | read / 50ms | literal SELECT run_id FROM launch.launches WHERE id=$1 in launch/launches_write.rs::record_launch_closed | new exact-primary-key lookup baseline; measure full fleet and nonfleet closure latency separately |
| fleet_record_launch_closure | read / 50ms | lifecycle_sql/record_launch_closure.sql; lifecycle_rows.rs::record_fleet_launch_closure; verified-exit delegates Some(true) | existing record_launch_closed.sql; compare null/false/true and late-replay cases, including exact-run/opened guard, monotonic CASE, and conditional registration release |
| fleet_complete_run_close | read / 50ms | lifecycle_completion.rs::close_owned_run; existing close_run.sql macro | unchanged close_run.sql; test rollback with fleet write failure |
| fleet_complete_state | read / 50ms | lifecycle_sql/complete_fleet.sql; lifecycle_completion.rs | existing set_fleet_state.sql targeting done, plus definition-save query if atomically staging; semantic guard cost measured separately |
| fleet_initialize_run_upsert | read / 50ms | lifecycle_start.rs existing upsert_run.sql macro | unchanged upsert_run.sql; verify transaction ordering and stop-wins no-write with integration tests |
| fleet_initialize_other_owner | read / 50ms | lifecycle_start.rs literal EXISTS on launch.fleets run_id + different fleet_uuid | new exact-run owner baseline using existing run_id index |
| fleet_record_open_run_surface | read / 50ms | lifecycle_sql/record_open_run_surface.sql; lifecycle_launch.rs::record_surface_run | existing upsert_run.sql workspace/tab branch, but refuses missing/closed runs and never changes closed_at |
| fleet_update_open_run | read / 50ms | lifecycle_sql/update_open_run.sql; lifecycle_launch.rs::record_run | existing upsert_run.sql metadata branch, but new query refuses closed/missing runs and never reopens them |
| fleet_stopping_constraint | migration / 5s | migration 20261014070100_fleet_stopping.sql | existing constraint/table cardinality; measure DDL transaction, lock wait, validation scan and relevant beneficiaries |

Measure lock acquisition and complete transaction latency separately from each statement's
EXPLAIN time. Hold the same fixture and parameter classes before and after, including
unknown/false/true closure, late replay, pending launches, namespace characters `%`/`_`,
and more than 200 unresolved roots. The old limited launch listing is not an equivalent
comparator for the new complete enumeration. Do not report empty synthetic tables as
representative evidence. Admission can hold SHARE through backend opening; measure real
contention and stop wait separately, with bounded scratch-only concurrency.

The parameterized `starts_with` pending-receipt predicate may not exploit the existing
index like equality does. Inspect its actual plan before adding an index or claiming a
budget pass. Use the stricter 5 ms hot-path budget if production usage puts an admission
query on a capture-sensitive path; do not relax a budget because of a slow measurement.

Rehearse migration `20261014070100_fleet_stopping.sql` on a fresh pre-change scratch copy.
Its replacement CHECK takes a schema lock and validates existing rows. Record validation
time, lock wait, cardinality and exact checksum, including contention with held admission
locks. Mutation rehearsals use BEGIN/ROLLBACK through the supported script; successful
full-method commit costs need separate controlled scratch measurements. Preserve actual
fingerprints and verdicts in the required evidence commit, and add these groups to the
post-deploy remeasurement list. No performance verdict or exemption is fabricated here.


### 9.1 Managed command admission and watchdog workload

Representative performance remains NOT RUN. The following additional runtime SQL is part of
the same outstanding G6 measurement requirement.

#### New statements

| ID | Source after integration | Literal SQL | Conservative tier |
|---|---|---|---|
| `managed_spawn_share` | `crates/mike/src/launch/fleet/owned_spawn_fence.rs`, `Identity::lock_inner` | `SELECT state, run_id FROM launch.fleets WHERE fleet_uuid = $1 FOR SHARE` | hot, 5 ms |
| `managed_receipt_insert` | `crates/mike/src/launch/fleet/managed_command.rs`, `begin` | `INSERT INTO launch.tree_checks (run_id, state, command, askers) VALUES ($1, $2, $3, ARRAY[$4::text]) RETURNING id` | hot, 5 ms |

The first statement is a new exact-UUID row-lock read. Measure AFTER as a genuinely new statement, then compare total old/new admission and spawn operation latency. The second replaces the former begin-tree-check insertion path; compare the actual former literal and operation from the review baseline, including any extra transaction round trips. Never invent a SELECT 1 baseline.

Both queue insertion and actual spawn use the SHARE lock. Queue insertion holds the local exact-run mutex and database lock through INSERT and commit. Actual heavy-command and managed auxiliary Git spawn holds the same locks across synchronous spawn and commit. The lifecycle transition takes a conflicting fleet-row lock, so a long spawn/admission transaction can delay stop. Measure lock-held wall time as well as SQL execution, include many simultaneous queue admissions and a concurrent stop, and report p50/p95/p99 stop wait separately from the SQL tool's best-of-25 measurement.

`Identity::lock` bounds the pool acquisition, begin and SELECT at two seconds. Receipt insertion and transaction commit each have a separate two-second bound. These are failure-containment deadlines, not acceptable performance budgets. An insertion timeout occurs before commit and drops the transaction; a commit timeout retains a blocked unknown-outcome registration.

#### Existing statements reused on new paths

- `PersistHandle::find_fleet` is called by `Identity::still_active` every two seconds per live context. The caller bounds each verification at two seconds and cancels on failure. Use the exact existing SQL from the candidate persistence source; measure fleet UUID lookup and total load for the largest representative live command count, including parked admission waiters.
- Existing completion `finish_tree_check`/receipt read paths remain the terminal record operations. Include durable cancellation receipt writes and failure/retry behavior in the operation comparison.
- `begin_for_runner` and `begin_baseline` call existing fleet/run reads before the same-transaction SHARE check. These reads are informative only; the locked exact UUID/run/state check is the spawn authority.

Copy full relevant `launch.fleets` and `launch.tree_checks` history with constraints, triggers, indexes, and real askers/command payload distributions. Include stopped/wrong-run/absent fleet refusals, ordinary and large askers/command values, simultaneous admissions for one row, separate fleet rows, delayed terminal receipt persistence, and a stop blocked behind an admission transaction. Observe only approved scratch databases. Do not use lock-sensitive EXPLAIN ANALYZE writes on live data.
