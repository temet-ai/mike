---
title: Codex projection storage
status: draft
created: 2026-10-13
updated: 2026-10-13
summary: Trusted Codex receipt capture, generation-isolated projection application, full-content reads and the limits of database-only recovery.
---

# Scope and status

C02b adds storage around the [C02a decoder and item reducer](CODEX_EVENT_PROJECTION.md)
and [C01 runtime evidence contract](RUNTIME_CONTRACT.md). Its source is
`crates/mike/src/store/persistence/runtime/codex/`; table definitions are in the
[runtime schema reference](SCHEMA.md#domain-runtime).

The module is linked and additive migration
`20261014070000_codex_projection_receipts.sql` is prepared in the migration directory.
All 44 authored tests now run in the existing integration harness: 19 codec/validation
cases and 25 database cases. All 44 passed on 2026-10-13 with disposable PostgreSQL
configured, and final full-workspace/all-target Clippy with warnings denied passed.
The monolithic library-test harness remains unlinked locally after repeated SIGKILLs
near 7.7 GiB RSS; these focused results are not a full-workspace test-gate claim.
Migration performance rehearsal and real-data query performance remain NOT RUN. The
owner's existing draft-publication exception leaves performance outstanding before merge;
it is not a passing G6 verdict.

This layer supplies public Rust `PersistHandle` methods and DTOs for a trusted adapter.
It adds no REST/MCP route, transport, process launch, RPC request registry, history
fetcher, lifecycle current-state reducer or fleet-reader migration. No live account,
agent, app-server or model is exercised or enabled. Managed Codex launch remains closed.

## Trusted admission and atomic capture

`CodexIntakeContext::new` owns C02a's traffic class and optional complete RPC scope
(typed string-or-integer request ID, method, requested thread and turn), plus a
non-nil connection epoch, nonnegative frame ordinal and `ClassificationProvenance`.
The context and admission-policy versions are both 1. Provenance values are
`ReviewedNotification`, `ReviewedRpcResponse` and `ReviewedOrdinaryRoute`.

These fields record the adapter's assertion. The public constructor and serde support
do not authenticate its caller or prove that a route was reviewed. A future external
endpoint must not accept these DTOs as user/provider-supplied capture authority. Never
classify an entire app-server output channel as ordinary content, or infer trust from
provider JSON, a parse failure or an unmatched response.

`capture_codex_receipt(CodexReceiptInput)` calls `observe` before opening a database
transaction. Authentication, ambiguous routing and unclassified traffic retain C02a's
exclusion rules; an excluded result carries only `ExclusionReason`, with no stored raw
receipt or context. An ordinary classification cannot override authentication or
response-correlation exclusions. Do not log the input or debug-dump excluded frames.

For an admitted observation the transaction:

1. Locks the namespace's `codex_intake_state` row, requiring a registered Codex namespace.
2. Looks up `(namespace_id, ingestion_id)` and compares the full observation and original
   context. An exact retry returns `Unchanged` with the same event ID and capture sequence.
   Changes to bytes, envelope (including receive time), parsed interpretation, RPC scope,
   traffic class, provenance, epoch or ordinal conflict without overwriting the receipt.
3. Checks transport order, inserts the raw event, increments the namespace capture
   counter and inserts context, then commits them together.

A failed context write rolls back the raw row and counter update. Projection occurs in
a separate transaction, so a projection failure does not discard committed raw evidence.
The caller must preserve ingestion UUID, original envelope and context across retries;
payload equality and provider item IDs are not deduplication keys.

The existing C01 `append_runtime_observation` remains available. Its raw-only rows have
`context`, `capture_sequence` and `transport_gap` set to `None` when read here. They are
unclassified legacy evidence, excluded from automatic Codex projection. Calling trusted
capture with an existing legacy receipt conflicts; no API silently attaches trust or
imports old rows. A concurrent legacy insert likewise cannot be reclassified by capture.

## Three different orders

- `runtime.events.id` is a raw-row identifier and inspection sort key. Identity allocation
  is not commit order, provider time or a projection checkpoint.
- `capture_sequence` is the namespace-local committed order of admitted raw-plus-context
  transactions. The intake-row lock is held until commit or rollback, so a later committed
  sequence cannot overtake an earlier outstanding sequence. Projection advances this prefix.
- `(connection_epoch, frame_ordinal)` is adapter-supplied transport evidence. Reusing or
  regressing an ordinal for a different receipt fails. A forward skip is admitted with
  `transport_gap = true`; exact receipt retries bypass this new-receipt order check.

The first retained receipt in an epoch has `transport_gap = false`, even when its ordinal
is greater than zero. False means no detected gap between retained receipts, not proof
that the stream began at zero, every frame was observed, or excluded frames were captured.
A true gap can include intentionally excluded traffic. There is no cross-epoch continuity
proof, repair, transport reordering or completeness upgrade. A recorded gap does not
stop projection or automatically set every affected item's reconciliation flag. Capture
sequence cannot recover provider order if the adapter submits frames incorrectly.

## Application and generation isolation

`begin_codex_projection_generation(namespace_id, generation_id)` creates an empty
`building` generation pinned to projector version 1 and state-codec version 1. Reusing
the same generation ID verifies namespace and supported versions; it does not reset state.

`apply_next_codex_projection(generation_id)` locks that generation, accepts only `building`
or `active`, and selects its next committed admitted receipt in capture-sequence order.
It loads the original context itself, decodes the stored observation and validates batch,
receipt and item-namespace associations. It does not accept an arbitrary caller-created
projection batch as database truth.

One transaction writes the complete effect journal, reduces all item effects in original
effect-ordinal order, saves affected item states, records the application and advances the
checkpoint. Effects and item states are inserted in set-based batches. Repeated item keys
within one receipt are reduced sequentially in memory before saving one state per key.
Every effect of a newly applied receipt receives `ReceiptApplication::Unapplied`; the
ledger prevents whole-receipt replay, rather than incorrectly skipping later effects.
The application FK from effects is deferred until commit.

`Applied` records projected, partial, unsupported, malformed or excluded decode status,
all issues and effect count. Valid effects in a partial batch survive. A handled receipt
with no effects can still advance the checkpoint. Infrastructure, codec or invariant
errors roll back effects, state, application and checkpoint together. There is no automatic
skip of a failing receipt. `CaughtUp` means no next committed eligible receipt was visible
for that generation at that call, not a complete or permanently finished provider stream.

New generations replay from empty state; no generation reads another generation's item
state or applications. `activate_codex_projection_generation` acquires locks in intake,
head, then target-generation order. It checks that the building generation's checkpoint
matches the locked intake boundary and that no admitted receipt lacks an application.
It atomically retires the old generation, activates the candidate and switches the head.
Replay itself does not hold the intake lock. Failed promotion leaves the old head intact.
Reactivating the current head returns `Unchanged`; retired generations cannot resume writes.

Read `active_codex_projection_generation(namespace_id)` to resolve a head, then pass that
explicit generation ID to data reads and retain it across pages. Reads never fall back to
another generation. Item/effect/application/issue reads, application and promotion reject
unsupported projector/codec versions; generation metadata lookups still expose their
stored version fields. Head lookup does not automatically advance projection.

## Full evidence and item-state codec

`codex_projection_effects` journals all five effect kinds: `item`, `lifecycle`, `signal`,
`usage` and `billing`. Each full serialized observation retains its receipt reference,
JSON pointer and evidence origin. Indexed thread/turn/item fields are validated opaque
identifiers, not synthesized Mike sessions. Raw receive time is available through the
referenced receipt; no producer timestamp is replaced with receive time.

`StoredProjectedItemV1` is a persistence-only codec. It stores the full key, snapshot,
terminal/reconciliation flags, first/latest/snapshot evidence, and separate display and
streamed text collections. Each text channel has a tag, explicit optional index and full
text. Reasoning content/summary require nonnegative indices; the other four channels
(message, plan, command output and file output) reject indices. Duplicate channels,
unsupported versions, unexpected or missing codec fields, key mismatches and nil or
cross-namespace evidence receipts fail decoding instead of losing data.

Provider-bearing documents use PostgreSQL `json`, with serialized text cast directly to
JSON; originals remain `BYTEA`. Full document reads use `::text` and decode in Rust. This
preserves escaped NUL content without JSONB conversion or SQL extraction of arbitrary
provider text. The strict persistence DTO does not erase unknown nested provider fields.
A materialized snapshot can combine retained older fields with newer ones; its snapshot
pointer is not per-field provenance. Use the effect journal and raw receipts to inspect
the derivation. The reducer's terminal-state and reconciliation semantics remain C02a's.

## Read API and pagination

All page sizes must be 1–1000. Limits bound whole rows, never the bytes within a row.
Methods return vectors rather than an opaque continuation token; derive the next cursor
from the last returned row, and continue until an empty page for a bounded inspection.

| Method | Scope and continuation |
|---|---|
| `runtime_receipt(ReceiptId)` | Full raw observation, optional original context and sequence/gap metadata |
| `list_runtime_raw_events(RawEventPage)` | Namespace; `after_event_id`, initially 0 |
| `list_codex_unclassified_receipts(RawEventPage)` | Same raw-page query restricted to missing context; caller selects a Codex namespace |
| `codex_projected_item(generation_id, &ItemKey)` | Exact generation and full namespace/thread/turn/item key |
| `list_codex_items(CodexItemPage)` | Generation/thread; `(first_capture_sequence, turn_id, item_id)` |
| `list_codex_projection_effects(CodexEffectPage)` | Generation/thread, optional one of five kinds; `(capture_sequence, effect_ordinal)` |
| `list_codex_usage_observations` / `list_codex_lifecycle_observations` | Same effect page, with the named kind forced |
| `codex_projection_application(generation_id, event_id)` | Full application status, issues and effect count |
| `list_codex_projection_issues(generation_id, after_sequence, limit)` | Applications with a nonempty issue array; last capture sequence |

Use the generic effect read for signal and billing observations. Item ordering preserves
first capture sequence, with turn/item IDs breaking ties; later item updates do not reorder
rows. Individual calls do not provide a cross-page snapshot: active item contents may
change while paging. Raw ID pagination can miss a lower-ID transaction that commits after
a page is read, so it is not a commit-complete recovery feed. Projection completeness uses
admitted capture sequence, never that raw inspection cursor.

Usage reads expose reported samples: `total` and `last` stay separate, and Missing, Null,
Invalid and explicit zero retain their meanings. There is no summing, counter differencing,
reset inference or parent/child aggregation. Billing credits and nullable USD micros remain
separate. Configured model does not establish actual model, and unknown timing/cost stays
unknown. Lifecycle and signal rows are observations, not a current-thread/turn state.

## Recovery, cancellation and remaining acceptance

Committed receipts and generation progress permit a caller to resume database projection.
There is no worker loop, queue, spool, retry scheduler, request cancellation, live-history
freshness cutoff or transport reconnect policy. A dropped/failed uncommitted transaction
does not intentionally publish partial state, but no cancellation/crash durability test
has run. If a commit's outcome is uncertain, inspect the stable receipt/application and
retry the same operation identity; do not allocate a new ingestion UUID to guess success.

The schema permits `failed` generations, but no public method marks a generation failed,
cancels it, deletes it or restores a retired generation. Stop invoking apply to pause a
building generation, and resume that same ID if its state and versions remain supported.
An apply error does not itself mark it failed. Keep the current generation readable while
investigating; do not edit context, effects or checkpoints to bypass an invariant.

Authored coverage includes codec round trips/errors, exclusion, exact/conflicting replay,
raw/context rollback, full raw reads, all effect variants, unknown usage, terminal items,
application rollback, concurrent intake/application, generation isolation and promotion.
The tier migration preserved all 44 test names and 233 assertions (230 normalized
expressions unchanged; three mechanical private-validator call-route substitutions); it added no ignored/disabled cases, new test binary or Cargo manifest change.

| Tier | Test location and scope |
|---|---|
| 19 codec/validation integration tests | `crates/mike/tests/integration/runtime_codex/{codec_tests,tests_validation}.rs`: codec round trips/errors, intake/batch validation and closed-pool errors; no database connection |
| 25 database integration tests | `crates/mike/tests/integration/runtime_codex/`, registered as `runtime_codex` in the existing `crates/mike/tests/integration.rs` harness: intake, atomicity, generations, observations, reads and database error cases |

The integration cases use the existing isolated ephemeral helper through
`runtime_codex/test_support.rs`; feature-gated `test-seams` wrappers expose the small
internal operations required by those fixtures. DB cases can return early when the helper
supplies no database, so a successful test-run summary alone must not be called DB coverage.
All 44 integration cases passed on 2026-10-13 with no ignored cases. The 25 database
cases used disposable PostgreSQL through the configured helper; the other 19 require
no database connection. Final strict workspace/all-target Clippy passed. These results
do not establish live compatibility or a successful full library-test link.

Before merge, finish validating the module/migration, run the applicable repository
gates and record actual executed-versus-unrun DB cases. Follow
FLEET_PERFORMANCE_VALIDATION.md for owner-run real-data
query and DDL measurements. C02c still needs history fetching and freshness reconciliation;
C02d needs runtime-aware readers; C03 needs authorized managed transport/control and live
compatibility evidence. This storage layer does not complete those acceptance criteria.
