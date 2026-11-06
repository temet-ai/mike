---
title: Codex event projection foundation
status: active
created: 2026-10-13
updated: 2026-11-06
summary: Pure, credential-safe Codex app-server observation decoding and item reduction, with explicit evidence and replay boundaries.
---

# Scope

`runtime::codex` is the C02a in-memory foundation. It consumes C01
`RuntimeObservation` values and projects typed facts without starting a process,
calling a service, writing a database, or changing a runtime account. Managed
Codex fleet launch remains unsupported. This module does not complete C02 or
C03 and does not supply wire-level model request/response capture. The separate
[C02b storage layer](CODEX_PROJECTION_STORAGE.md) supplies trusted receipt context,
raw reads and transactional projection; its integration/validation status is recorded
there. This module remains pure and does not own those database operations.

The implemented shapes were checked against generated official Codex 0.159.2
stable and experimental app-server JSON schemas, particularly `RequestId`,
`ItemStartedNotification`, `ItemCompletedNotification`, `ThreadItem`,
`ThreadItemEntry`, `ThreadTokenUsageUpdatedNotification`,
`GetAccountTokenUsageResponse`, `DynamicToolCallParams`, and lifecycle and
side-stream notifications. Tests use explicitly synthetic local fixtures, not
captured production traffic or a real Codex executable. Unknown provider fields
and future variants remain inspectable; this is not a generated exhaustive
protocol client or a claim of live compatibility.

# Intake before ordinary raw persistence

Call `observe(envelope, raw_payload, intake_context)` before appending an
ordinary raw runtime event. Pass the same `IntakeContext` into
`decode_observation`. The caller owns stable namespace/ingestion UUIDs, source
metadata, receive timestamps, and any existing envelope associations. The
module neither allocates provider identities nor guesses ownership.

`IntakeContext` supplies optional request correlation and a local `TrafficClass`:
`Unknown` (the default), `OrdinaryContent`, or `Authentication`. The class is
trusted local metadata, never a field read from provider JSON. A transport must
establish it independently; a pending request or failure to parse is not itself
proof of ordinary content.

Default admission is deliberately narrow:

- Reviewed notification/request methods consumed by this decoder are admitted.
  Unknown item variants and unknown fields within those methods remain admitted.
- Responses are admitted only with an exact string-or-integer request ID and a
  correlation for `thread/read`, `thread/items/list`, `thread/turns/list`, or
  `account/usage/read`. Unknown RPCs and other control responses, including
  `config/read`, are outside this content contract. Their successful, error,
  and malformed correlated frames are excluded with no payload.
- Unknown/future notification methods and unparseable frames are excluded as
  `UnclassifiedTraffic`, unless independently classified `OrdinaryContent`.

An admitted frame becomes a boxed C01 observation with its original byte vector
unchanged. Explicit ordinary classification permits full retention of malformed
JSON, invalid UTF-8, whitespace, and escaped NUL content. Duplicated non-routing
keys inside ordinary content remain byte-identical; the optional parsed JSON is
an interpretation and does not replace evidence. Malformed extracted provider
identifiers become projection failures without preventing raw storage. Invalid
caller-supplied C01 envelopes remain contract errors for admitted frames.

Authentication is outside this capture boundary. An `Authentication` class,
recognized auth/login/logout/OAuth/recovery method, or attestation generation
excludes the frame even if mistakenly classified `OrdinaryContent`. Correlated
auth responses are excluded even when malformed. A top-level routing audit can
recognize an auth method before a malformed tail and does not rely on
last-key-wins interpretation. Excluded results contain only a reason, no bytes,
credentials, request ID, provider method name, or other frame content.

Duplicated top-level routing keys (`method`, `id`, `result`, or `error`) and
mixed method/response envelopes fail closed, including under `OrdinaryContent`.
A top-level `result` or `error` is response-bearing even when `method` is null.
Parsed responses with missing/mismatched correlation are also excluded; an
ordinary label does not override these checks. Direct decoding of an ambiguous
C01 frame reports malformed with no effects.

This is a deliberate ordinary-content admission contract, not full-stream
fidelity or a transport. It does not implement framing, a request registry,
credentials handling, a queue, persistence retries, or a spool. Malformed bytes
alone cannot prove their traffic class. The future transport must classify
traffic correctly, retain the same context for projection, and never use
ordinary raw capture as an authentication log.

# Projection and evidence

`decode_observation` validates C01 raw/parsed consistency and returns one
`ProjectionBatch`. It does not mutate or duplicate the full raw observation.
Each normalized effect cites the namespace/ingestion receipt, evidence origin,
and a JSON pointer into the original parsed frame. History item pointers use
`/result/data/<index>/item`, preserving the original response's RPC ID, page
metadata, and sibling item timing in the raw observation.

The batch status distinguishes projected, partial, unsupported, malformed, and
excluded results. Diagnostic codes and pointers carry no provider content.
A malformed history entry does not suppress valid siblings. Unknown item types
produce generic item snapshots plus an explicit unsupported-type issue.
Unrecognized methods remain raw-visible without invented effects.

Provider IDs are case-sensitive opaque strings. Nonempty strings with no NUL
are accepted without UUID parsing or normalization. Items are keyed by
namespace, thread, turn, and item. A process ID, tool call ID, request ID, and
shared session-tree ID are separate concepts. Conflicting associations with an
existing C01 envelope suppress the effect and report a conflict, including a
session-tree mismatch independent of a matching thread ID. Invalid optional
session/parent/fork identifiers remain explicit invalid fields with issues,
never valid identity associations.

`Field<T>` represents `Missing`, `Null`, `Value(T)`, and `Invalid(original JSON)`.
This is intentionally tolerant of partial fields and schema evolution. A
projected batch means its selected observations were decoded, not that every
provider field passed exhaustive schema validation. Unknown fields remain in
the complete provider payload. Invalid known fields remain explicit rather
than becoming defaults.

Implemented observations include:

- User content arrays, assistant text, plans, emitted reasoning, and compaction
- Command properties, output deltas, process IDs, status, optional exit/duration,
  and terminal-input signals
- MCP and custom tool item facts, progress, server tool-call requests, and
  separate function-output items
- File-edit payloads, output deltas, patch snapshots, and turn-wide diff signals
- Thread and turn lifecycle, approval/input signals, turn plans, routing changes
- Token usage samples and separately labelled per-thread billing estimates

Signals preserve full provider payloads. Plan updates and turn diffs are
snapshots; they are not appended as text deltas or counted as independent edits.
A file-edit result is not evidence of the final disk or commit state.
Connection-scoped `command/exec/outputDelta` and `process/outputDelta` are not
misclassified as agent command items. They remain unsupported unbound evidence.

Correlated `thread/items/list` responses decode entries only. Correlated
`thread/read` responses decode thread metadata only; nested turn/item history
remains raw and does not establish projected history completeness.
`thread/turns/list` is admitted as reviewed content but currently returns an
unsupported projection with raw response intact. No pagination
cursor is consumed, checkpoint advanced, membership deleted, or history fetch
performed by this module.

# Item reduction and replay

`reduce_item(current, observation, receipt_application)` returns a new value.
The caller supplies `Unapplied` or `AlreadyApplied`; the item does not carry an
ever-growing set of previously applied receipts. The caller must decide replay
once per complete receipt batch, apply all its effects, then mark that receipt
applied atomically. [C02b storage](CODEX_PROJECTION_STORAGE.md) implements that
boundary with generation-isolated applications, an effect journal and item state;
this pure reducer is not a substitute for the transaction.

C02b's `CodexIntakeContext` persists the adapter-owned traffic classification and
complete RPC scope/correlation separately from the provider payload. A stored JSON-RPC
response contains an ID, not the request method or trusted traffic class. Replay
restores the original context rather than guessing it from payload fields or treating
provider JSON as capture authority. Legacy raw-only rows remain unclassified and
cannot silently acquire trust. The persistence layer owns this representation; the
pure module continues to consume borrowed `IntakeContext` values.

The unit tests use a caller-owned in-memory receipt set to demonstrate that a
previously applied older receipt cannot append again after a newer one. Two
distinct receipts containing identical adjacent text both append. Provider item
IDs, payload equality, and hashes are not raw-event or projection dedupe keys.

Reduction rules:

- Full snapshot text replaces provisional display text; it is never appended
  onto a delta assembly. Streamed text remains separately available as evidence.
- A completed plan can differ from the concatenated plan stream.
- A completed item cannot regress on a late started snapshot. A late delta
  remains streamed evidence, leaves terminal display intact, and requests
  reconciliation.
- Contradictory live terminal snapshots flag conflict and deterministically use
  the latest supplied live snapshot. Original observations remain the source
  of both versions.
- Compatible history enriches missing fields without regressing terminal state
  or falsely treating the different snapshot source as a conflict. Overlapping
  contradictory content or timestamps preserve the terminal item and request
  reconciliation.
- Absent top-level snapshot fields preserve known provider fields and displayed
  text. Present structured values, including tool results, replace atomically;
  omitted nested keys are not invented from an older result. Explicit null
  remains distinguishable and can clear displayed text.
- Present reasoning summary/content arrays replace their indexed display
  channels, including removal of old indices. Streamed reasoning remains
  separate evidence.
- A late started snapshot cannot erase text already observed in a live stream.
- Item order is its immutable first evidence. Receipt UUIDs are not sort clocks.

`snapshot_evidence` identifies the most recently applied or enriching snapshot;
`latest_evidence` identifies the latest observation, including ignored late
updates. A late delta cannot replace the authoritative snapshot pointer. A
materialized snapshot can combine fields retained from older snapshots with
new fields. The pointer is not per-field provenance; recover earlier field
sources from the original receipt effects/raw observations. No per-field
provenance ledger is implemented here.

The future reconciler must establish response-dispatch cutoffs and live-update
ordering before applying history snapshots to active items. This foundation
cannot decide freshness across a live/history race. It also does not implement
turn/thread lifecycle reduction, disk durability, cross-process ownership,
exactly-once transactions, or a gap-free recovery guarantee. C02b supplies the separate
database transaction and replay boundary; it does not solve live/history freshness,
transport completeness or lifecycle current-state reduction.

# Usage, cost, model and time

Usage effects retain provider `total` and `last` separately. There is no sum,
implicit difference, clamping on counter decrease, reset assumption, or
parent/child/fork aggregation. `modelContextWindow` remains capacity. Missing
`cacheWriteInputTokens` does not become its schema default of zero.

Billing responses require an explicitly requested thread and matching returned
thread ID. Missing or null `threadUsage` stays unavailable. Credits and USD
micros remain separate integer fields; zero USD is explicit zero, not a
substitute for unknown cost. Account-wide summaries are not seat usage. Group
payloads are preserved without estimating absent tokens or model names.

Thread model is `configured_model`; `actual_model` remains missing. Routing
signals preserve from/to/reason without claiming universal per-inference model
telemetry. Creating-thread CLI version is distinct from the C01 running-runtime
version. Turn timestamps remain seconds and item timestamps remain milliseconds;
receipt time is never substituted for producer time.

# Checks and remaining acceptance

Focused unit tests cover positive admission and local traffic classification,
auth exclusion, duplicate/mixed routing, ordinary malformed/raw fidelity, opaque
IDs, partial/unknown variants, common categories, correlated response pointers,
identical adjacent deltas, caller-owned replay, snapshot precedence, terminal
conflicts, missing/null/zero usage and cost, configured model, and time units.
Run them through the repository test wrapper with the `runtime::codex` filter.
These tests do not contact a database, launch an app-server, or use a model.

C02b's durable raw reads and transactional replay/application implementation is
linked; see its [storage contract](CODEX_PROJECTION_STORAGE.md). Real-data
performance has not been run.

`runtime::codex::history` (`crates/mike/src/runtime/codex/history/`) holds the pure side of
history reconciliation: `HistoryRequest`/`HistoryQuery`/`HistoryIdentity` build a page
request, `HistoryPage` and `HistoryEntry`/`HistoryTurn`/`HistoryItem` decode the response,
and `next_history_step` with `HistoryCursorLedger` decides the next cursor action or the
`HistoryBlocker`. It does not fetch anything or decide freshness across a live/history race.
Resumable reconciliation against a live app-server and scripted RPC tests remain C02c work.
C02d needs runtime-aware fleet readers. C03 needs authorized managed transport, control and
live compatibility evidence before Codex launches can be enabled.
