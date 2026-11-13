---
title: Runtime identity and observation contract
status: active
created: 2026-10-13
updated: 2026-11-12
summary: The bounded C01 identity and raw-event persistence foundation, its Claude compatibility boundary, and the request-dependent readers still awaiting normalized runtime data.
---

# Runtime identity and observation contract

This contract covers the Codex work tracked in
[issue #18](https://github.com/temet-ai/mike/issues/18) and the common observation
layer in [#181](https://github.com/temet-ai/mike/issues/181). Common types live in `crates/mike-base/src/runtime/`; persistence lives in
`crates/mike-base/src/store/persistence/runtime/`. The additive migration
`20261013130000_runtime_identity_events.sql` creates the `runtime` schema. See
[the schema reference](SCHEMA.md#domain-runtime) for columns and indexes.

## Compatibility and launch boundary

Omitted fleet runtime selectors still mean `claude`. Explicit `claude` takes the same plan and
launch path; existing Claude session UUIDs are unchanged. Both control-seat and worker definitions
carry the selector through planning. Unknown selector names fail deserialization.
`codex` uses the independent `mike-codex` app-server client through Mike's managed
launcher. There is no fallback from a Codex selection to a Claude process.

Existing `sessions.sessions.session_uuid` remains the Mike identity exposed to callers, with
`sessions.id` as the internal bigint join key. This contract does not migrate Claude sessions,
add synthetic captured requests, or infer model, usage, price or tool results from an event.
The existing prompt-assistant Codex runner is not connected to this fleet contract.

## Durable namespaces and thread identity

`RuntimeNamespace` has a persistent UUID, a runtime kind and a durable scope key. An adapter must
reuse the namespace across process restarts and reconnects; a PID or an app-server lifetime is
not a namespace. Namespace identity and `(runtime kind, durable key)` are unique. Provider IDs
are nonempty, case-preserving opaque strings, not UUIDs. Whitespace inside or around a nonempty
ID is preserved. NUL is rejected in indexed identifier fields because PostgreSQL text cannot
represent it; the original event payload can still retain that evidence.

A `RuntimeThreadBinding` maps one Mike session to one `(namespace, provider thread ID)`. The
provider session ID is optional and deliberately nonunique: a seat and its native children may
share it. A native child has its own Mike session and an owning-seat UUID; it is not implicitly
an independently schedulable seat. Parentage and fork ancestry are separate namespace/thread
references. Their namespaces must exist, but their referenced threads need not have arrived.
Thus a child can be stored before its parent binding. Self-parent, self-fork and a native child
owning itself are rejected. Cross-thread ancestry cycles are not reconciled by this foundation.

Bindings are immutable assertions. Registering an identical namespace or binding returns
`Unchanged`; reusing an identity or durable key with different content fails. Missing Mike
sessions or owners fail instead of silently creating them. Unknown parent/fork references can
be retained verbatim, but enriching a previously stored null relationship needs a future
explicit reconciliation operation. Raw events can be stored while identity is unresolved.
Deleting a Mike session cascades its binding; deleting an owning seat clears the resolved
bigint owner FK. The original binding document retains the historical owning-seat UUID.

## Original runtime events

This section describes the retained legacy raw-event API and historical data.
New managed, hook and telemetry observation uses the bounded common contract
below; raw events are not a prerequisite for visibility.

`RuntimeObservation` separates original bytes, optional parsed JSON, and a versioned envelope.
Every byte of the payload is stored in `BYTEA`: invalid UTF-8, malformed JSON, duplicate JSON
keys, whitespace and unknown event variants all survive. The parsed JSON projection is
secondary and cannot replace the bytes. JSON `null` has a parsed value and is distinct from
an unavailable projection. A supplied projection that differs from parsing the raw bytes fails
validation. No captured model-request record is synthesized.

The envelope records its positive version, namespace, ingestion UUID, receive timestamp,
optional provider timestamp, optional provider session/thread/turn/item/event IDs and source
cursor. Source metadata records kind, optional source and runtime versions, adapter and adapter
version, fidelity and completeness. Fidelity/completeness are open strings; `unknown` is a
valid statement. Unknown envelope/source properties are retained. Future positive envelope
versions survive unchanged when they contain the required core fields. Extension properties
cannot overwrite defined fields. A structurally incompatible future envelope requires a new
adapter rather than guessed metadata.

Envelope and projection columns use PostgreSQL `JSON`, not `JSONB`, so valid escaped NULs in
unknown JSON fields do not reject a byte-preserving observation. Identity documents use
`JSONB`; their identifier fields are validated before writes. Event JSON parameters are sent as
serialized text and cast directly to `JSON`, avoiding the driver's default `JSONB` parameter type. Missing parsed JSON means parsing
was unavailable, not that the provider emitted nothing. Missing usage, monetary cost, timing,
observed model or context information remains unknown. Requested model is not observed model.

### Replay semantics

The raw append key is `(namespace UUID, ingestion UUID)`. The adapter assigns and persists that
UUID when recording an observation and reuses the entire envelope on retry, including the
original receive time. An exact retry returns `Unchanged`. A reused key with any changed bytes,
parsed projection or envelope field is an explicit conflict and leaves the original intact.
Concurrent identical retries produce one insert and one unchanged result through a unique
constraint and subsequent exact comparison. No update-on-conflict rewrites event content.

Provider item/event IDs and cursors are evidence, not raw-event uniqueness constraints. A
stream can contain multiple updates to the same item. Two deliveries with different ingestion
UUIDs remain separate observations; the same ingestion UUID in different namespaces is scoped
separately. Semantic projection deduplication and history reconciliation belong to C02.

## Request-dependent reader audit

The roster, fleet seat rows and session summary now read common observation
views, with a Claude adapter over its existing capture tables. Their API output
also exposes runtime, source, fidelity, freshness, control capability and nullable
usage. Session detail exposes paginated common item metadata with content
availability. The remaining broader fleet analytics in this audit are outside
the three-reader acceptance boundary of #181 and retain their existing coverage;
this work does not claim complete intervention or fleet-acceptance parity (#25).

These readers still need normalized runtime projections before they can truthfully include
Codex seats. The current C01 slice leaves their Claude behavior intact; it does not claim
runtime-neutral reporting because raw storage is present.

| Reader | Current assumption | Required normalized facts |
|---|---|---|
| `fleet/fleet_seat_rows.sql`, `crates/mike-launch/src/launch/fleet/status.rs` | Captured requests/responses drive activity, context, HTTP status and cache; terminal-backend presence supplies liveness fallback | Runtime lifecycle, observed activity time, freshness and reconciliation state; optional scoped context/usage, provider health; terminal presence separate from execution |
| `crates/mike-launch/src/launch/fleet/staff.rs` | Warm siblings need recent captured HTTP traffic and a successful response | Adapter readiness and runtime-specific warm/resume capability with evidence |
| `workspace/roster.rs` | Missing request can imply uncaptured and exclude a session | Explicit observation source, capture coverage and registration/liveness independent of wire traffic |
| `fleet_correlation/nodes.rs`, `rehydrate/session_summary.rs`, `rehydrate/session_filtered.rs`, `rehydrate/session_head.rs` | Request-derived model, activity, counts, prompt, latency and lifecycle | Configured versus observed model; event/turn activity and optional time/usage; evidence source and freshness |
| `fleet_correlation/actor.rs` | Claude tool projections identify files and dispatched agents; session existence acts as captured-session lookup | Tool kind plus original provider name, file evidence and attribution, native child identity and observed status |
| `fleet/tree_results.sql`, `fleet/slow_commands.sql` | Claude Bash tool records and response completion time stand in for command execution | Command start/completion timestamps, arguments, output, outcome and provenance; absent timing stays unknown |
| `fleet/background_ends.sql` | Captured tool-use IDs join Claude background work items | Runtime-scoped tool item identity, background task state and completion time with reconciliation evidence |
| `fleet/tree_changes_since.sql` | Claude tool names and timestamps supply attribution candidates | Runtime-neutral file-change evidence with provider item/turn linkage, time semantics and source |
| `fleet/run_record_sql/seats.sql`, `fleet/run_record_sql/cargo_repeats.sql` | Captured tools, metrics, commands and commit timing | Normalized command/tool outcome, scoped usage, evidence-backed commit association |
| `fleet/run_record_sql/forks.sql` | Template launch ancestry stands in for a fork | Separate launch template lineage, runtime fork lineage, native parentage and owning seat |
| `launch/launches_read.rs`, `launch/prompt_turn_seen.sql` | Claude Read tools and captured prompts indicate brief consumption | Runtime-specific brief delivery/consumption evidence, kept separate from transport acceptance |
| Session/mailbox projections using `sent_request_id` | Request correlation can stand in for delivery | Stable message identity and separate accepted/consumed/acted/reconciled stages, without an invented Claude request |

The paths in this table are under `crates/mike/src/store/persistence/` unless noted; fleet
control logic also consumes the same observations under `crates/mike-launch/src/launch/fleet/`.
Normalized facts must carry source, version, fidelity and completeness. Cumulative versus
delta usage and parent-versus-child scope must be explicit before any aggregation.

## Common observations and bounded persistence

`SessionObservation` carries an opaque runtime name and a versioned source, fidelity,
completeness and observation time. It can update lifecycle, requested/observed model,
scoped usage, a model call or one terminal item. The store owns atomic receipts and
fact application; a duplicate retries safely, a conflicting observation ID fails,
and quota refusal is explicit. Independent clocks keep delayed lifecycle, control
and usage updates from being erased by newer unrelated activity. Managed control
is not inferred from passive observation.

Common state, model calls and item metadata live in `runtime.session_facts`,
`runtime.model_calls` and `runtime.session_items`. Common reader views adapt Claude's
existing tables without copying its traffic. Codex protocol decoders live in
`mike-codex`; the Mike bridge maps their facts to this contract. A test adapter with
another runtime name uses the same store and roster/fleet/session readers.

Usage has explicit basis and scope. Provider thread totals replace prior snapshots;
they are not added to per-call totals. Missing token components and billing remain
unknown. Item content is explicitly `not_collected`; terminal tool/command status
does not imply a full transcript or raw capture. APIs return paginated whole item
metadata rows and observation provenance.

Ordinary sessions advertise hook-boundary delivery only after the synchronous hook
is installed and observed. Local queue wakeups require a verified Codex home/thread,
use a fixed mailbox-read notice, and record queued/failed/uncertain status durably.
Neither queue success nor hook output acknowledges peer mail. The recipient reads
its inbox and calls `ack_session_message` after recording or acting on the message.
Peer text remains data under [the injection trust policy](INJECTION_TRUST.md).

See [Codex capture](CODEX_CAPTURE.md) for budgets, expiry and evidence availability.

## Historical foundation validation and broader remaining work

Offline fixtures cover legacy selector behavior, opaque ID round trips, child/fork separation,
invalid identities, unknown metadata/version retention, malformed raw bytes, exact replay
comparison and closed-pool errors without a database connection. Conditional database fixtures
cover durable namespace conflicts, unresolved ancestry, raw round trips (including escaped
NUL), concurrent replay, same-item updates, missing references and absence of request writes.
These tests use the repository's isolated test database helper, never the capture database.

Test fixtures are not evidence of actual Codex emissions. Before Codex can be called supported, the normalized
reader work and supported-version emitted behavior must be validated. C02 ingestion/history,
C03 managed lifecycle, C04 durable mixed-runtime mail and C05 child/usage reconciliation remain
separate work, followed by C06/C07 observation/wire fidelity and C08 fleet/UI acceptance.

## Performance validation

Performance validation of the C01 statements and the changed fleet query groups, and gate G6,
remain NOT RUN. Correctness fixtures are not a performance verdict.
