---
title: Database Schema Reference
status: active
created: 2026-05-01
updated: 2026-11-06
summary: Reference for mike's multi-schema Postgres database — tables, columns, FKs, indexes, views, and triggers. Every table a migration creates has a section (223 sections); the migrations are the source of truth.
---

# Database Schema Reference

**Scope:** this is a maintained reference, not an exhaustive generated schema dump. It
gives every table a migration creates its own section (223 in all). Where a definition here disagrees
with `crates/mike/src/store/persistence/migrations/`, the ordered migrations are authoritative.

The proxy's Postgres database captures, persists, and exposes every Claude API request that flows through the
proxy. It exists so operators and future Claude sessions can inspect full-fidelity request/response history,
tool activity, system prompts, diagnostic findings, and agent lifecycles without ever touching the proxy's
in-memory state.

**Schema baseline:** one file — `crates/mike/src/store/persistence/migrations/20260604000001_baseline.sql`
— replaced 104 incremental migration files in a clean-break squash (2026-06-04). The baseline defined the
schema as of that date; dozens of incremental migrations have landed on top of it since (new schemas, tables,
columns, and views). The current schema is the baseline plus every migration after it, not the baseline file alone.

**Structure:** tables and vocabulary live in the schema that owns their domain. The read-API
schema is `mike`: migration `20260718000000_rename_claude_proxy_schema_to_mike.sql` renamed
`claude_proxy`, carrying its views and trigger functions with it. Many tables have a bigint
identity primary key; others use natural or composite keys, including `session_judges.attempts`,
`eval.axis_labels`, `sync.cursor` and `sync.id_map`. Read each table's declared key.

Schemas:

| Schema | Domain |
|---|---|
| `projects` | Project metadata |
| `sessions` | Session lifecycle |
| `runtime` | Durable provider identity and original runtime observations |
| `messages` | Deduped message-body store + role vocab |
| `system_prompts` | Deduped system-prompt store + block-layout vocab |
| `rules` | Filter rules + volatile-line patterns + kind/action/source vocab |
| `requests` | Request capture + per-request facts + request vocab |
| `responses` | Response capture + stream chunks + response vocab |
| `tools` | Tool invocations + results |
| `agents` | Agent definitions, dispatches, instances + source vocab |
| `overrides` | Prompt overrides + revisions + mode/source vocab |
| `memory` | Operator turns, chunks, lessons + outcome/stage/kind vocab |
| `prompt_kinds` | Prompt-kind classification vocab + per-turn classification facts |
| `session_judges` | Per-session LLM-judge outcome verdicts + outcome vocab |
| `steering` | Steerer definitions + intervention attribution + authority/mode/verdict vocab |
| `mailbox` | Inter-session message delivery + reminder injection tracking |
| `workspace` | Session registration/liveness + append-only coordination-event ledger |
| `launch` | Terminal-launcher attempts, warm templates, runs, backend validations |
| `config` | Runtime-mutable feature toggles + open-keyspace settings overrides |
| `sync` | Local-DB-fallback resync bookkeeping (cursor + id remap) |
| `llm` | Outbound LLM call accounting and captured inputs |
| `remote_control` | Remote Control channels, frames and session links |
| `work` | Work threads, criteria, items and attention tracking |
| `eval` | Evaluation bundles, labels, runs, rates and alarms |
| `mike` | Read-API views and row-change notification functions |

---

## Domain: `projects`

### `projects.projects`

One row per distinct working directory. Root of the cascade chain: project deletion cascades to sessions,
which cascade to requests and everything dependent on them.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `project_path` | `TEXT NOT NULL` | Filesystem path or `(no project)` sentinel |
| `display_name` | `TEXT NOT NULL` | Human-readable label |
| `first_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `pinned` | `BOOLEAN NOT NULL DEFAULT FALSE` | Exempts from TTL eviction |
| `ttl_override` | `INTERVAL NULL` | Per-project TTL override |
| `pin_created_at` | `TIMESTAMPTZ NULL` | When project was pinned |
| `ttl_override_set_at` | `TIMESTAMPTZ NULL` | When TTL override was set |
| `git_repo` | `TEXT NULL` | Canonical grouping key: basename of the repo working-directory root, captured from the Claude Code env block (`extract::request::extract_git_repo`) |
| `custom_name` | `TEXT NULL` | Operator-set friendly name, distinct from the capture-derived `display_name`; never written by capture upserts or canonicalization |

`git_repo` (migration `20260625000006_projects_git_repo_canonical.sql`) collapses the duplicate project rows a single repo used to accumulate (worktrees, differing parent dirs, display-name truncation) onto one canonical identity per repo. It is a low-entropy natural key, not a content hash, so `idx_projects_git_repo_unique` is a legitimate unique index. Known limitation: two distinct repos sharing a directory basename collapse to one identity — accepted by operator 2026-06-22, no git remote URL available in captured traffic to disambiguate further.

**Indexes:**
- `idx_projects_last_seen`: `last_seen DESC` (BTree) — session rehydration
- `idx_projects_pinned`: `id WHERE pinned = TRUE` (BTree partial) — eviction sweep
- `idx_projects_git_repo_unique`: `git_repo WHERE git_repo IS NOT NULL` (BTree unique partial) — one project row per repo

---

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `project_uuid` | uuid NOT NULL DEFAULT gen_random_uuid() |

---

### `projects.machines`

Added by `20260920000008_project_knowledge.sql`. One row per machine that has registered a checkout
of a project: the identity mike keeps in its state directory (`machine_identity::MachineIdentity`,
`machine.json`). Upserted when a location is registered (`POST /api/knowledge/locations`,
`store/persistence/project_knowledge/locations.rs`) or a repository is adopted
(`project_knowledge/repo_adoption.rs`), and merged by the startup legacy-identity merge
(`store/persistence/machine_merge.rs`). Read by the knowledge health report
(`project_knowledge/health.sql`) and the project detail rehydrate (`rehydrate/project_detail.rs`).

| Column | Type | Purpose |
|---|---|---|
| `machine_uuid` | `UUID` | Primary key; the machine's durable identity |
| `name` | `TEXT NOT NULL` | The machine's display name; CHECK not blank |

**Constraints:**
- CHECK `btrim(name) <> ''` (unnamed)

---

### `projects.locations`

Added by `20260920000008_project_knowledge.sql`. A machine-qualified checkout of a logical project;
"paths do not define portable project identity" (table comment). Upserted on `(machine_uuid,
root_path)` by `store/persistence/project_knowledge/locations.rs` (`POST /api/knowledge/locations`,
`define_knowledge_project`) and `repo_adoption.rs`; `last_seen` is bumped on every re-registration
and by knowledge document writes (`project_knowledge/documents.rs`). Referenced by
`sessions.sessions.location_uuid`, `memory.knowledge_sources.location_uuid` and
`memory.lesson_file_conflicts.location_uuid` (CASCADE). Read by the knowledge read/search/health
queries, `mailbox.session_real_project_id()` (a session's project from its working folder),
`change_record/checkouts_with_working_sessions.sql`, the work-thread import
(`work_threads/import.rs`, `project.rs`) and `project_merge.rs`.

| Column | Type | Purpose |
|---|---|---|
| `location_uuid` | `UUID DEFAULT gen_random_uuid()` | Primary key |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` (no ON DELETE action) |
| `machine_uuid` | `UUID NOT NULL` | FK → `projects.machines(machine_uuid)` (no ON DELETE action) |
| `root_path` | `TEXT NOT NULL` | The checkout's root on that machine; CHECK not blank |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Last registration or knowledge write that named this location |
| `origin_url` | `TEXT` | The checkout's normalized git remote, recorded by the machine that holds it (`workspace/checkouts.rs`, each knowledge scan); how a checkout on another machine finds its project. NULL for a folder that is not a checkout root |

**Constraints:**
- UNIQUE `(machine_uuid, root_path)`
- CHECK `btrim(root_path) <> ''` (unnamed)

**Indexes:**
- `locations_project`: `project_id` (BTree)
- `locations_origin_url`: `origin_url` (BTree, partial: not null)

---



## Domain: `sessions`

### `sessions.sessions`

One row per Mike session UUID, preserving existing Claude Code UUIDs. Provider runtime identities are mapped
through `runtime.threads`. The session UUID lives here; all child tables reference `sessions.id`
(bigint). The `mike.session_summary_v` view re-exposes `session_uuid::text` as `session_id` for the
REST/MCP API.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_uuid` | `UUID NOT NULL UNIQUE` | Mike session UUID; existing Claude UUIDs unchanged |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `title` | `TEXT NULL` | Best-effort session title from title-sidecar request |
| `pinned` | `BOOLEAN NOT NULL DEFAULT FALSE` | Exempts from TTL eviction |
| `ttl_override` | `INTERVAL NULL` | Per-session TTL override; NULL = inherit project/global default |
| `first_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Updated on every terminal request |
| `last_status` | `TEXT NULL` | Last request status |
| `pin_created_at` | `TIMESTAMPTZ NULL` | When session was pinned |
| `ttl_override_set_at` | `TIMESTAMPTZ NULL` | When TTL override was set |
| `overview` | `TEXT NULL` | Two-paragraph narrative overview of the session, LLM-generated or operator-authored |
| `overview_generated_at` | `TIMESTAMPTZ NULL` | Timestamp the LLM generated `overview`; NULL when `overview` is unset or operator-authored |

**`overview`/`overview_generated_at` tri-state** (migration `20260702000002_add_session_overview.sql`; also
documented on `PersistHandle::set_session_overview`): `overview NOT NULL, overview_generated_at NULL` =
operator-authored (manual override); `overview NOT NULL, overview_generated_at NOT NULL` = LLM-generated at
that timestamp; both NULL = no overview yet. Distinct from the per-turn stop-response digest served by
`GET /api/sessions/{id}/overview` (`rehydrate.rs`) — that endpoint answers "what did each turn say", this
column pair answers "what happened in this session" as a whole. Neither column is indexed: a two-paragraph
free-text field has no equality/range predicate use.

**Columns absent** (now computed in `mike.session_summary_v`): `request_count`, `last_path`,
`last_models`.

**Indexes:**
- `brin_sessions_last_seen`: `last_seen` BRIN(32) — time-range session scans
- `idx_sessions_pinned`: `id WHERE pinned = TRUE` (BTree partial) — eviction sweep
- `idx_sessions_project_last_seen`: `(project_id, last_seen DESC)` (BTree) — project → session list
- `idx_sessions_title_trgm`: `title WHERE title IS NOT NULL` (GIN trgm partial) — title fuzzy search

**Trigger:** `trg_sessions_row_changed` — AFTER UPDATE; NOTIFY `proxy.row_changed`

---

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `location_uuid` | uuid REFERENCES projects.locations(location_uuid) |

### `sessions.session_goals`

Added in migration `20260629000003_session_goals.sql` (`next_steps` column added in
`20260629000005_session_goals_next_steps.sql`). One goal per session, set by the steering subsystem or MCP
(`set_session_goal`). 1:1 extension of `sessions.sessions` — the PK is the FK, so resolving a session's goal
is a single indexed PK lookup; no separate unique index is needed.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `BIGINT PRIMARY KEY` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `goal` | `TEXT NOT NULL` | Goal text |
| `set_by` | `TEXT NOT NULL DEFAULT 'mcp'` | Origin of the goal write |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `next_steps` | `TEXT NOT NULL DEFAULT ''` | Free-text next-steps note alongside the goal |
| `role` | `TEXT NULL` | Free-text role the session declares for itself (e.g. `rust-developer`, `coordinator`); open set, no enum. `GET /api/fleet/graph` resolves a node's role from this column first, falling back to `agents.agents.name` only when the session is a dispatched subagent |
| `display_name` | `TEXT NULL` | Free-text handle the session declares for itself (e.g. `observer-1`, `rust-dev-alpha`); open set, **not unique**. What peers see instead of a uuid prefix, and what mailbox `name:` addressing resolves against |
| `instructions` | `TEXT NULL` | The standing brief this session was given — distinct from `goal` (the current objective under that brief) and `next_steps` (what it will do next) |

Added in migration `20260713140000_session_goals_role.sql`. `role` is declared, never inferred from goal/title
text — a goal paragraph is not a role. Free-text because `agent_dispatches.subagent_type` already spans 20+
definitions and a spawned peer session (coordinator, reviewer, observer, …) has no dispatch row at all, so it
must declare its own role through this column.

`display_name` and `instructions` added in migration
`20260920000001_session_identity_name_instructions.sql`, which turns this row into the session's full
declared **identity**: name, role, instructions, goal, next_steps. Identity extends this row rather than
growing a second 1:1 table because `role` — the field `role:` mailbox fan-out already resolves against —
was here first; splitting name off would have meant two writes and a join for one concept.

`display_name` carries **no uniqueness constraint**, deliberately. Two live sessions may declare the same
name, and `resolve_name_recipient` reports `Ambiguous` with the candidate uuids rather than letting a
constraint violation surface at write time — the same shape session-id-prefix resolution already uses. Both
name and role resolution match case-insensitively, served by the partial functional indexes below.

`instructions` is fenced as DATA wherever a peer's row is rendered (roster, briefing, fleet graph). It is the
most instruction-shaped text in any of those payloads and is still only a report of what someone else was
told.

**Indexes:**
- `idx_session_goals_display_name_lower`: `lower(display_name) WHERE display_name IS NOT NULL` (BTree partial functional) — mailbox `name:` resolution
- `idx_session_goals_role_lower`: `lower(role) WHERE role IS NOT NULL` (BTree partial functional) — `role:` fan-out (`resolve_role_recipients`), which matches case-insensitively as of the same migration

### `sessions.session_analyzer_overrides`

Added in migration `20260712000001_session_analyzer_overrides.sql`. Backs `PATCH
/api/sessions/{id}/analyzers/{name}` (issue temet-ai/mike-private#174): a per-session enable/disable override for one
analyzer (a category analyzer or `weasel_intercept`), overlaid on top of
`AnalyzerRegistry::snapshot()`'s global state by `GET /api/sessions/{id}/analyzers`. Many-per-session
(one row per overridden analyzer), unlike the 1:1 `session_goals` shape — mirrors `steering.enrolments`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `analyzer_name` | `TEXT NOT NULL` | Stable analyzer identifier (e.g. `overselling_analyzer`, `weasel_intercept`) |
| `enabled` | `BOOL NOT NULL` | Per-session override value |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `uq_session_analyzer_overrides_session_analyzer` (UNIQUE): `(session_id, analyzer_name)` — also
  serves the "all overrides for this session" lookup via its leftmost column, so no separate
  `session_id`-only index is needed.

---

## Domain: `runtime`

Added by `20261013130000_runtime_identity_events.sql`. This is the C01 identity and raw-event
foundation; managed Codex execution and normalized reader coverage remain pending. See
[`RUNTIME_CONTRACT.md`](RUNTIME_CONTRACT.md) for validation, exact replay semantics and the
request-dependent reader audit. These tables never synthesize `requests.requests` rows.

### `runtime.namespaces`

| Column | Definition / purpose |
|---|---|
| `id` | UUID primary key; durable across runtime process restarts, non-nil |
| `runtime_kind` | TEXT NOT NULL; `claude` or `codex` |
| `durable_key` | TEXT NOT NULL; nonempty stable provider scope, never a PID |
| `identity` | JSONB NOT NULL; full validated `RuntimeNamespace` |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() |

Unique `(runtime_kind, durable_key)`. Identical retries are unchanged; conflicting IDs or scopes
are rejected. Existing Claude sessions do not need a namespace backfill.

### `runtime.threads`

| Column | Definition / purpose |
|---|---|
| `session_id` | BIGINT PRIMARY KEY; FK to `sessions.sessions(id)`, ON DELETE CASCADE |
| `namespace_id` | UUID NOT NULL; FK to `runtime.namespaces(id)` |
| `provider_thread_id` | TEXT NOT NULL; opaque, nonempty and case-preserving |
| `provider_session_id` | TEXT NULL; deliberately nonunique across parent and child threads |
| `owning_seat_id` | BIGINT NULL; FK to `sessions.sessions(id)`, ON DELETE SET NULL |
| `native_parent_namespace_id`, `native_parent_thread_id` | Nullable namespace UUID FK and opaque thread TEXT pair; both present or both absent |
| `fork_namespace_id`, `fork_thread_id` | Independent nullable namespace UUID FK and opaque thread TEXT pair |
| `identity` | JSONB NOT NULL; full binding with public Mike UUIDs and original ancestry |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() |

Unique `(namespace_id, provider_thread_id)`. Parent/fork references do not require a thread row,
so child-first ingestion retains unresolved ancestry. The namespaces themselves must exist.
Separate partial indexes cover owning-seat, native-parent and fork references. Bindings are
immutable through the runtime persistence API; a changed retry fails. The binding document
retains historical ownership when deletion clears the resolved `owning_seat_id` FK.

### `runtime.events`

| Column | Definition / purpose |
|---|---|
| `id` | BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY; stable observation ordering |
| `namespace_id` | UUID NOT NULL; FK to `runtime.namespaces(id)` |
| `ingestion_id` | UUID NOT NULL, non-nil; durable raw-ingestion retry identity |
| `provider_thread_id` | TEXT NULL; no binding FK, so evidence may arrive first |
| `envelope` | JSON NOT NULL; version, identity, source/fidelity/completeness, timestamps and unknown metadata |
| `raw_payload` | BYTEA NOT NULL; original bytes, including malformed or non-UTF-8 payloads |
| `parsed_payload` | JSON NULL; secondary parsed projection; SQL NULL differs from JSON `null` |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() |

Unique `(namespace_id, ingestion_id)` is the replay key. An identical retry returns unchanged;
changed bytes or any changed envelope/projection field produce a conflict without overwriting
the original. Provider item/event IDs are never raw-event uniqueness keys. The envelope carries
optional provider session/thread/turn/item/event IDs, source cursor and provider timestamp,
plus receive time, positive envelope version and source/runtime/adapter version metadata.
Unknown source/envelope properties and future positive versions retain their values.

`JSON` permits escaped NULs in unknown metadata/projections that `JSONB` would reject. Original
bytes remain authoritative; parsed JSON cannot preserve whitespace or duplicate keys. Index
`idx_runtime_events_thread_order` covers `(namespace_id, provider_thread_id, id)`. Runtime event
retention/reconciliation and normalized fleet-reader projections are not implemented here.

### `runtime.codex_intake_state`

Added by `20261014070000_codex_projection_receipts.sql` (C02b), together with the six
`runtime.codex_*` tables below and unique index `idx_runtime_events_namespace_id` on
`runtime.events (namespace_id, id)`, which lets those tables reference an event together with its
namespace. The seven tables store trusted Codex receipts and the item projection built from them,
one projection generation at a time, so a new projector version can rebuild beside the active one
and replace it. They are scaffolding with no production callers: written and read only through
`crates/mike/src/store/persistence/runtime/codex/` (`intake.rs`, `generation.rs`, `apply.rs`,
`effects.rs`, `state.rs`, `projection_reads.rs`), whose only callers are
`crates/mike/src/runtime/codex/` and the tests under `tests/integration/runtime_codex/`; no proxy
path, worker, REST route or MCP tool reaches them.

This table holds one row per namespace: the last capture sequence handed out for that namespace's
receipts (`intake.rs`).

| Column | Type | Purpose |
|---|---|---|
| `namespace_id` | `UUID` | Primary key; FK → `runtime.namespaces(id)` (no ON DELETE action) |
| `last_capture_sequence` | `BIGINT NOT NULL DEFAULT 0` | Last capture sequence issued; CHECK `>= 0` |

---

### `runtime.codex_receipt_context`

The transport context of one captured Codex event (one row per `runtime.events` row admitted as a
receipt). `capture_sequence` is the projection order within the namespace. Scaffolding, no
production callers (see `runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `event_id` | `BIGINT` | Primary key; with `namespace_id`, FK → `runtime.events(namespace_id, id)` |
| `namespace_id` | `UUID NOT NULL` | |
| `capture_sequence` | `BIGINT NOT NULL` | Projection order within the namespace; CHECK `> 0` |
| `context_version` | `INTEGER NOT NULL` | CHECK `> 0` |
| `connection_epoch` | `UUID NOT NULL` | One app-server connection; CHECK non-nil |
| `frame_ordinal` | `BIGINT NOT NULL` | Position within the connection; CHECK `>= 0` |
| `transport_gap` | `BOOLEAN NOT NULL` | Frames may be missing before this one |
| `context_document` | `JSON NOT NULL` | The full receipt context |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- FK `(namespace_id, event_id)` → `runtime.events(namespace_id, id)` (no ON DELETE action)
- UNIQUE `(namespace_id, capture_sequence)`
- UNIQUE `(namespace_id, connection_epoch, frame_ordinal)`
- UNIQUE `(namespace_id, event_id, capture_sequence)` (the composite FK target for the application and effect tables)

---

### `runtime.codex_projection_generations`

One projection generation: a projector version's pass over a namespace's receipts. A generation is
`building` until activated, then `active`, `retired` or `failed`. Scaffolding, no production callers
(see `runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `generation_id` | `UUID` | Primary key; CHECK non-nil |
| `namespace_id` | `UUID NOT NULL` | FK → `runtime.namespaces(id)` (no ON DELETE action) |
| `projector_version` | `INTEGER NOT NULL` | CHECK `> 0` |
| `state_codec_version` | `INTEGER NOT NULL` | CHECK `> 0` |
| `state` | `TEXT NOT NULL` | CHECK `IN ('building', 'active', 'retired', 'failed')` |
| `last_applied_capture_sequence` | `BIGINT NOT NULL DEFAULT 0` | CHECK `>= 0` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `activated_at` | `TIMESTAMPTZ NULL` | NULL until active |

**Constraints:**
- UNIQUE `(namespace_id, generation_id)` (the composite FK target for the tables below)

---

### `runtime.codex_projection_heads`

One row per namespace naming its active generation. Scaffolding, no production callers (see
`runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `namespace_id` | `UUID` | Primary key; FK → `runtime.namespaces(id)` (no ON DELETE action) |
| `active_generation_id` | `UUID NULL` | With `namespace_id`, FK → `runtime.codex_projection_generations(namespace_id, generation_id)`; NULL while no generation is active |

---

### `runtime.codex_projection_applications`

One row per event a generation applied, with the outcome of applying it. Scaffolding, no production
callers (see `runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `generation_id` | `UUID NOT NULL` | |
| `namespace_id` | `UUID NOT NULL` | |
| `event_id` | `BIGINT NOT NULL` | |
| `capture_sequence` | `BIGINT NOT NULL` | CHECK `> 0` |
| `status` | `TEXT NOT NULL` | CHECK `IN ('projected', 'partial', 'unsupported', 'malformed', 'excluded')` |
| `issues` | `JSON NOT NULL` | |
| `effect_count` | `INTEGER NOT NULL` | Effects produced; CHECK `>= 0` |
| `applied_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- PRIMARY KEY `(generation_id, event_id)`
- UNIQUE `(generation_id, capture_sequence)`
- FK `(namespace_id, generation_id)` → `runtime.codex_projection_generations(namespace_id, generation_id)`
- FK `(namespace_id, event_id, capture_sequence)` → `runtime.codex_receipt_context(namespace_id, event_id, capture_sequence)`

---

### `runtime.codex_projection_effects`

The effects one application produced, in order. The FK to its application is DEFERRABLE INITIALLY
DEFERRED so an application and its effects insert in one transaction. Scaffolding, no production
callers (see `runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `generation_id` | `UUID NOT NULL` | |
| `namespace_id` | `UUID NOT NULL` | |
| `event_id` | `BIGINT NOT NULL` | |
| `effect_ordinal` | `INTEGER NOT NULL` | Position within the application; CHECK `>= 0` |
| `capture_sequence` | `BIGINT NOT NULL` | |
| `effect_kind` | `TEXT NOT NULL` | CHECK `IN ('item', 'lifecycle', 'signal', 'usage', 'billing')` |
| `thread_id` | `TEXT NOT NULL` | CHECK not blank |
| `turn_id` | `TEXT NULL` | CHECK not blank when present |
| `item_id` | `TEXT NULL` | CHECK not blank when present |
| `observation` | `JSON NOT NULL` | |

**Constraints:**
- PRIMARY KEY `(generation_id, event_id, effect_ordinal)`
- FK `(generation_id, event_id)` → `runtime.codex_projection_applications(generation_id, event_id)` DEFERRABLE INITIALLY DEFERRED
- FK `(namespace_id, generation_id)` → `runtime.codex_projection_generations(namespace_id, generation_id)`
- FK `(namespace_id, event_id, capture_sequence)` → `runtime.codex_receipt_context(namespace_id, event_id, capture_sequence)`

**Indexes:**
- `idx_codex_effects_thread_order`: `(generation_id, thread_id, capture_sequence, effect_ordinal)` (BTree) — a thread's effects in capture order
- `idx_codex_effects_item_order`: `(generation_id, thread_id, turn_id, item_id, capture_sequence, effect_ordinal) WHERE item_id IS NOT NULL` (BTree partial)
- `idx_codex_effects_kind_order`: `(generation_id, thread_id, effect_kind, capture_sequence, effect_ordinal)` (BTree)

---

### `runtime.codex_item_state`

The reduced state of each item in a generation. Scaffolding, no production callers (see
`runtime.codex_intake_state`).

| Column | Type | Purpose |
|---|---|---|
| `generation_id` | `UUID NOT NULL` | |
| `namespace_id` | `UUID NOT NULL` | |
| `thread_id` | `TEXT NOT NULL` | CHECK not blank |
| `turn_id` | `TEXT NOT NULL` | CHECK not blank |
| `item_id` | `TEXT NOT NULL` | CHECK not blank |
| `terminal` | `BOOLEAN NOT NULL` | |
| `reconciliation_required` | `BOOLEAN NOT NULL` | |
| `first_capture_sequence` | `BIGINT NOT NULL` | CHECK `> 0` |
| `latest_capture_sequence` | `BIGINT NOT NULL` | CHECK `>= first_capture_sequence` |
| `state_codec_version` | `INTEGER NOT NULL` | CHECK `> 0` |
| `state_document` | `JSON NOT NULL` | |

**Constraints:**
- PRIMARY KEY `(generation_id, thread_id, turn_id, item_id)`
- FK `(namespace_id, generation_id)` → `runtime.codex_projection_generations(namespace_id, generation_id)`

**Indexes:**
- `idx_codex_items_stable_order`: `(generation_id, thread_id, first_capture_sequence, turn_id, item_id)` (BTree) — a thread's items in first-seen order

---








### Codex projection tables

Each `runtime.codex_*` table has its own section below.

### `messages.roles` (vocab)

| `id` | `code` |
|---|---|
| 1 | `user` |
| 2 | `assistant` |
| 3 | `system` |

### `messages.messages`

Per-message canonical store. One row per unique canonical-JSON `messages[]` entry, keyed by SHA-256.
Eliminates the N² redundancy of repeating the full conversation tail in every request body. `message_sha256`
is a UNIQUE natural key via CHECK; `id` is the bigint surrogate PK. `role_id` FK replaces the old `role TEXT`
+ CHECK.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `message_sha256` | `TEXT NOT NULL` | 64-char lowercase hex SHA-256 of canonical bytes |
| `body` | `JSONB NOT NULL` | Full canonicalized JSON |
| `byte_size` | `INTEGER NOT NULL` | Length of canonical bytes |
| `role_id` | `BIGINT NOT NULL` | FK → `messages.roles(id)` |
| `first_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_messages_last_seen`: `last_seen DESC` (BTree) — stale-message eviction

---

## Domain: `system_prompts`

### `system_prompts.blocks` (vocab)

Natural positional key — the value IS the index into the wire `system[]` array. Deliberately diverges from
the `id BIGINT IDENTITY` convention: the classifier strip logic indexes `system[N]` by this value directly.

| `block_index` (PK) | `code` | `description` |
|---|---|---|
| 0 | `billing_header` | Harness-injected per-process billing line. Volatile every process. |
| 1 | `identity_preamble` | System metadata / identity preamble; may carry `cache_control`. |
| 2 | `role_body` | Agent identity prompt (10–80 KB). Hashed for prompt-class identity. |

### `system_prompts.system_prompts`

One row per unique system prompt (content-addressed by SHA-256 of `system[2].text`). The 30 kB main-agent
blob stored once; all requests that share a prompt point here.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `prompt_sha256` | `TEXT NOT NULL` | 64-char lowercase hex content-address |
| `blocks` | `JSONB NOT NULL` | The parsed 3-block array verbatim |
| `first_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `last_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Constraints:**
- `prompt_sha256_is_64_hex`: CHECK `prompt_sha256 ~ '^[0-9a-f]{64}$'`

**Indexes:**
- `idx_system_prompts_sha256`: `prompt_sha256` (BTree) — system prompt lookup

---

### `system_prompts.session_prompts`

Added in migration `20260704000001_session_prompts_capture_ledger.sql`. Per-conversation-start capture
ledger, distinct from `system_prompts.system_prompts` (the class-SHA dedup table). Grain: one row per
conversation start — one `lane='main'` row per session (the first main-lane request carrying a system
array), plus one `lane='subagent'` row per subagent/workflow-subagent invocation with a system array and
`message_count == 1`. `blocks` is JSONB because it holds the full verbatim wire `system[]` array — a
schemaless capture payload, not domain objects to filter/join on. `content_sha256` is an informational
"same as previous" marker (computed over the volatile-stripped blocks); it is never a dedup key or join
target.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `request_id` | `BIGINT NOT NULL UNIQUE` | FK → `requests.requests(id)` ON DELETE CASCADE; the conversation-start request that carried this system array |
| `lane` | `TEXT NOT NULL` | `main` \| `subagent` (`session_prompts_lane_check` CHECK) |
| `subagent_persona` | `TEXT NULL` | Synchronous agent hint read off the wire before the agent matcher binds; may be set even when `agent_id`/`instance_id` are still NULL |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL; stamped by the agent matcher once it resolves the dispatch |
| `instance_id` | `BIGINT NULL` | FK → `agents.instances(id)` ON DELETE SET NULL; stamped by the agent matcher once it resolves the sub-agent instance |
| `blocks` | `JSONB NOT NULL` | Full verbatim wire `system[]` array |
| `content_sha256` | `TEXT NULL` | Informational "same as previous" marker (SHA-256 over the volatile-stripped blocks); never indexed directly, never a dedup key |
| `altered` | `BOOLEAN NOT NULL DEFAULT FALSE` | TRUE when the composer spliced `system[2]` (main/PrimaryFirst lane only) |
| `alteration_used` | `BOOLEAN NOT NULL DEFAULT FALSE` | Confirmed TRUE once the response shows the splice took effect |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Constraints:**
- `session_prompts_lane_check`: CHECK `lane IN ('main', 'subagent')`

**Indexes:**
- `idx_session_prompts_session_lane`: `(session_id, lane) INCLUDE (content_sha256)` (BTree) — `content_sha256` rides as an index-only payload, never its own index
- `idx_session_prompts_agent_id`: `agent_id WHERE NOT NULL` (BTree partial)
- `idx_session_prompts_instance_id`: `instance_id WHERE NOT NULL` (BTree partial)
- `uq_session_prompts_one_main_per_session`: `session_id WHERE lane = 'main'` (BTree unique partial) — enforces one main-lane row per session at the database level

---

## Domain: `rules`

### `rules.kinds` (vocab)

Seeded: `text_substring` (1), `xml_tag` (2), `lazy_section` (3), `inject_reminder` (4).

### `rules.actions` (vocab)

Seeded: `drop` (1), `replace` (2), `append` (3).

### `rules.sources` (vocab)

Seeded: `toml` (1), `ui` (2).

### `rules.rules`

Runtime CRUD, DB-only (authored via the inspect API / MCP `*_filter_rule` tools; no bootstrap-file write-through). `uuid` is the stable external identifier;
`id` is the bigint PK. `kind_id`, `action_id`, `source_id` are FK lookups (replacing CHECK constraints).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL` | External identifier for CRUD operations |
| `kind_id` | `BIGINT NOT NULL` | FK → `rules.kinds(id)` |
| `value` | `TEXT NOT NULL` | Substring or tag name to match |
| `action_id` | `BIGINT NOT NULL` | FK → `rules.actions(id)` |
| `text` | `TEXT NULL` | Replacement/appended text; NULL when `action='drop'` |
| `enabled` | `BOOLEAN NOT NULL DEFAULT TRUE` | |
| `source_id` | `BIGINT NOT NULL` | FK → `rules.sources(id)` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `project_path` | `TEXT NULL` | NULL = global rule (fires on every request); non-NULL = scoped rule (fires only when the request's project path matches exactly) |
| `sort_order` | `INTEGER NOT NULL DEFAULT nextval('rules.rules_sort_order_seq')` | Operator-controlled application order (ascending). Consumers order by `(sort_order ASC, id ASC)` — duplicates are permitted by design (no UNIQUE, to allow swap/reorder UPDATEs) |
| `trigger_token` | `TEXT NULL` | `lazy_section`/`inject_reminder` rules only: token (e.g. `!!CHROME!!`) whose appearance in assistant text re-injects the associated content as a `<system-reminder>`. NULL for every other kind |
| `inject_text` | `TEXT NULL` | For `lazy_section` rules: overrides the re-injected content when `trigger_token` fires, instead of the extracted section text. NULL falls back to extracting the section from the request body |
| `title` | `TEXT NULL` | Operator-authored display name for the rule. NULL when unset — consumers derive a fallback title from `kind`/`value`/`action` instead. Empty string is never stored; the API normalizes it to NULL |

**Trigger:** `trg_filter_rules_row_changed` — on `rules.rules`; NOTIFY `proxy.row_changed`

### `rules.volatile_line_patterns`

Operator-authored regex patterns applied to a block's text before computing `prompt_sha256`. Matching lines
are stripped for classification purposes only — bytes forwarded upstream are byte-identical to what the client
sent.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `name` | `TEXT NOT NULL` | Human-readable pattern name |
| `pattern` | `TEXT NOT NULL` | Regex pattern |
| `applies_to_block_index` | `INTEGER NOT NULL` | FK → `system_prompts.blocks(block_index)` |
| `enabled` | `BOOLEAN NOT NULL DEFAULT TRUE` | |
| `notes` | `TEXT NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_volatile_line_patterns_applies_to_block_index`: `applies_to_block_index` (BTree)

---

## Domain: `requests`

### Vocabulary tables

| Table | Seeded values |
|---|---|
| `requests.methods` | `GET`(1) `POST`(2) `PUT`(3) `PATCH`(4) `DELETE`(5) `HEAD`(6) `OPTIONS`(7) `TRACE`(8) `CONNECT`(9) |
| `requests.statuses` | `pending`(1) `streaming`(2) `complete`(3) `error`(4) |
| `requests.turn_types` | `user_prompt`(1) `tool_loop`(2) `continuation`(3) |
| `requests.turn_origins` | `primary`(1) `title_sidecar`(2) `subagent`(3) `advisor_sidecar`(4) `quota_probe`(5) `primary_first`(6) `monitor_sidecar`(7) `count_tokens_sidecar`(8) `web_fetch_sidecar`(9) `steered`(10) `workflow_subagent`(11) `next_input_suggestion_sidecar`(12). `subagent`/`workflow_subagent` are classified from the system-prompt persona / `running inside a workflow script` marker on **every** request in a sub-agent's lineage — including `tool_loop` continuations — not just its first turn, so cross-session mailbox/steerer reminders never land on a sub-agent's tool loop. |
| `requests.delta_change_kinds` | `added`(1) `removed`(2) |
| `requests.delta_removed_reasons` | `compaction`(1) `tail_rebuild`(2) `unknown`(3) |
| `requests.author_kinds` | `operator`(1) `harness`(2) `agent`(3) `unknown`(4); FK target of `requests.requests.author_kind_id` (NOT NULL DEFAULT 4) |

### `requests.requests`

Single plain table. **Two-phase write semantics:** a `pending` row is inserted at request-send time so child
FK rows can land during SSE streaming; the row is updated to `complete`/`error` at stream completion.

Enum text columns from earlier versions (`method`, `status`, `turn_type`, `turn_origin`) are now FK lookups
(`method_id`, `status_id`, `turn_type_id`, `turn_origin_id`). Error text moved to `requests.errors`.

`operator_turn_id` is the canonical turn-membership fact, stamped by the Rust classifier when a turn is opened
or extended. `agent_instance_id` and `operator_turn_id` are declared as plain bigint here; their FKs are added
in a trailing `ALTER TABLE` block (see FK Cycle section below).

`predecessor_request_id` and `request_number` are materialised at capture — an `UPDATE` inside the same
snapshot transaction that writes the `requests.messages` bridge, run unconditionally rather than gated on the
delta set being non-empty (an identical re-send still has a predecessor). The session-timeline listing and
`fetch_request_number` read the stored columns directly; the single-row detail path still recomputes the
predecessor live from the same lineage rule. Lineage scoping: for a `subagent` row the predecessor is the
prior request within the same `agent_instance_id`, which is a different fact from the dispatching primary
request the listing exposes as `dispatch_request_id`. Historical rows were populated by the one-shot
`mike backfill-predecessors` command; both columns are NULL on rows that never took the snapshot path.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` |
| `method_id` | `BIGINT NOT NULL` | FK → `requests.methods(id)` |
| `path` | `TEXT NOT NULL` | Request path (e.g. `/v1/messages`) |
| `model` | `TEXT NULL` | Model string from request params |
| `is_streaming` | `BOOLEAN NOT NULL` | Whether the request asked for SSE streaming |
| `message_count` | `INTEGER NOT NULL` | Number of messages in the request body |
| `is_tool_loop` | `BOOLEAN NOT NULL` | Whether request is a tool-loop follow-up |
| `status_id` | `BIGINT NOT NULL` | FK → `requests.statuses(id)` |
| `request_headers` | `JSONB NULL` | Raw request headers |
| `params` | `JSONB NOT NULL` | Full request params (model, max_tokens, etc.) |
| `last_user_message` | `TEXT NOT NULL` | Extracted last user message text |
| `human_prompt` | `TEXT NOT NULL` | Full extracted operator prompt text (no truncation) |
| `turn_type_id` | `BIGINT NOT NULL` | FK → `requests.turn_types(id)` |
| `turn_origin_id` | `BIGINT NOT NULL` | FK → `requests.turn_origins(id)` |
| `subagent_persona` | `TEXT NULL` | Extracted persona label when `turn_origin='subagent'` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Set at intercept time |
| `completed_at` | `TIMESTAMPTZ NULL` | Set at terminal write; NULL = in-flight |
| `persisted_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Wall clock when row was written to DB |
| `request_body` | `BYTEA NULL` | Full wire-form request body bytes |
| `filtered_request_body` | `BYTEA NULL` | Post-filter body bytes when filter was applied |
| `intercepted` | `BOOLEAN NOT NULL DEFAULT FALSE` | TRUE when response was suppressed and replaced by a correction request |
| `correction_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; self-referential |
| `agent_instance_id` | `BIGINT NULL` | Plain bigint; FK added via ALTER (see FK Cycle) → `agents.instances(id)` ON DELETE SET NULL |
| `operator_turn_id` | `BIGINT NULL` | Plain bigint; FK added via ALTER (see FK Cycle) → `memory.operator_turns(id)` ON DELETE SET NULL |
| `claude_code_agent_id` | `TEXT NULL` | Captured `x-claude-code-agent-id` header; present only on workflow sub-agent requests. Unindexed — low cardinality per session, not a join key |
| `predecessor_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; self-referential. Preceding request in the same session/agent-instance chain |
| `request_number` | `BIGINT NULL` | 1-based session-chronological ordinal, ordered by `(created_at, id)` |
| `transformation_manifest` | `JSONB NULL` | Opaque per-request `TransformationManifest` (`crates/mike/src/compress/manifest.rs`): ordered original→sent deltas keyed by SHA256 + byte ranges, never content bytes. Unindexed — read whole per request, never filtered on |

**Indexes:**
- `brin_requests_created_at`: `created_at` BRIN(32)
- `idx_requests_session_id`: `session_id` (BTree)
- `idx_requests_project_created_desc`: `(project_id, created_at DESC)` (BTree)
- `idx_requests_correction_request_id`: `correction_request_id WHERE NOT NULL` (BTree partial)
- `requests_agent_instance_id_idx`: `agent_instance_id WHERE NOT NULL` (BTree partial)
- `idx_requests_inflight_session`: `session_id WHERE status_id IN (1,2)` (BTree partial) — in-flight detection
- `idx_requests_model_created_at`: `(model, created_at DESC) WHERE model IS NOT NULL` (BTree partial)
- `idx_requests_session_completed`: `(session_id, completed_at) WHERE completed_at IS NOT NULL` (BTree partial)
- `idx_requests_status_created_at`: `(status_id, created_at DESC)` (BTree)
- `idx_requests_turn_origin_created_at`: `(turn_origin_id, created_at DESC)` (BTree)
- `idx_requests_operator_turn_id`: `operator_turn_id WHERE NOT NULL` (BTree partial)
- `idx_requests_session_created_id`: `(session_id, created_at, id)` (BTree) — backs the write-path `request_number` count and the read-path chronological ordering

**Trigger:** `trg_requests_row_changed` — AFTER INSERT OR UPDATE OR DELETE; NOTIFY `proxy.row_changed`

---

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `request_body_size` | integer |
| `request_envelope` | bytea |

### `requests.methods` (vocab)

Baseline lookup of HTTP methods. FK target of `requests.requests.method_id` and
`remote_control.calls.method_id` (the latter reuses it rather than adding a parallel vocabulary).
Deliberately not indexed from either side: the rows are never deleted.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Method name |

Seeded: `GET` (1), `POST` (2), `PUT` (3), `PATCH` (4), `DELETE` (5), `HEAD` (6), `OPTIONS` (7),
`TRACE` (8), `CONNECT` (9).

---

### `requests.statuses` (vocab)

Baseline lookup of request lifecycle states. FK target of `requests.requests.status_id`;
`idx_requests_inflight_session` treats ids 1 and 2 as in flight.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | State name |

Seeded: `pending` (1), `streaming` (2), `complete` (3), `error` (4).

---

### `requests.turn_types` (vocab)

Baseline lookup of turn types. FK target of `requests.requests.turn_type_id`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Turn type |

Seeded: `user_prompt` (1), `tool_loop` (2), `continuation` (3).

---

### `requests.turn_origins` (vocab)

Lookup of where a request came from (the primary thread, a sidecar, a sub-agent). FK target of
`requests.requests.turn_origin_id`. Seeded with ids 1–6 at baseline and extended one row per
migration since; every `INSERT` uses `OVERRIDING SYSTEM VALUE` and resets the sequence.
`subagent`/`workflow_subagent` are classified on every request in a sub-agent's lineage, not just
its first turn (see the vocabulary summary above).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Origin name |

Seeded: `primary` (1), `title_sidecar` (2), `subagent` (3), `advisor_sidecar` (4), `quota_probe`
(5), `primary_first` (6) (baseline); `monitor_sidecar` (7,
`20260609000002_add_monitor_sidecar_turn_origin.sql`); `count_tokens_sidecar` (8,
`20260609000003_add_count_tokens_sidecar_turn_origin.sql`); `web_fetch_sidecar` (9,
`20260610000002_add_web_fetch_sidecar_turn_origin.sql`); `steered` (10,
`20260613000002_steering_schema.sql`); `workflow_subagent` (11,
`20260618000001_add_claude_code_agent_id.sql`); `next_input_suggestion_sidecar` (12,
`20260702000001_add_next_input_suggestion_sidecar_turn_origin.sql`).

---

### `requests.author_kinds` (vocab)

Added by `20260811010000_requests_author_kind.sql` ([issue #105](https://github.com/temet-ai/mike-private/issues/105)):
who authored the turn — the operator, the Claude Code harness, or an agent. The same migration added
`requests.requests.author_kind_id BIGINT NOT NULL DEFAULT 4` (FK `requests_author_kind_id_fkey`,
added NOT VALID then validated; index `idx_requests_author_kind_id`), set in the same INSERT that
writes the request row (`store/persistence/request_write.rs`) and re-classified by the origin
backfill (`store/persistence/origin_backfill.rs`). Historical rows keep `unknown`; no backfill ran.
Read by the turn-fact classifier (`turn_facts/classify_load.rs`), the prompt stream
(`prompt_stream/j2_text.sql`), `dossier_parity_dispositions.sql` and the learning proposal queue.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Author kind |

Seeded: `operator` (1), `harness` (2), `agent` (3), `unknown` (4).

---






### `requests.messages`

Ordered FK bridge per request. One row per `(request, ord)` pair. The content SHA comes via JOIN to
`messages.messages`; there is no denormalized `message_sha256` mirror column.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `message_id` | `BIGINT NOT NULL` | FK → `messages.messages(id)` |
| `ord` | `INTEGER NOT NULL` | 0-based message position (CHECK >= 0) |

**Indexes:**
- `idx_request_messages_request_id`: `request_id` (BTree)
- `idx_request_messages_message_id`: `message_id` (BTree)

---

### `requests.deltas`

Per-request immutable delta fact table. One row per message change (added or removed) relative to the
predecessor primary request in the same session. Write-once at snapshot time. `predecessor_request_id` is
a real FK ON DELETE SET NULL (not a soft reference).

`change_kind_id` and `removed_reason_id` are FK lookups (replacing the old TEXT CHECK columns). `role`,
`byte_size`, and `body` are not stored on this table — every read (`fetch_request_deltas_by_id`) sources
them by JOIN on `message_id` against `messages.messages`/`messages.roles` instead, since they were
byte-identical to the message's own row on every sampled write and the duplicate copy accounted for 98.6%
of the table's row content.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `predecessor_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `message_id` | `BIGINT NOT NULL` | FK → `messages.messages(id)`; also the source of `role`/`byte_size`/`body` on read |
| `change_kind_id` | `BIGINT NOT NULL` | FK → `requests.delta_change_kinds(id)` |
| `ord` | `INTEGER NOT NULL` | Position in the request's message list (CHECK >= 0) |
| `removed_reason_id` | `BIGINT NULL` | FK → `requests.delta_removed_reasons(id)`; required on `removed` rows |
| `computed_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When delta was computed |

**Constraints:**
- `request_deltas_reason_only_on_removed`: `(change_kind_id=1 AND removed_reason_id IS NULL) OR (change_kind_id=2 AND removed_reason_id IS NOT NULL)`

**Indexes:**
- `idx_request_deltas_request_id`: `request_id` (BTree)
- `idx_request_deltas_session_id`: `session_id` (BTree)
- `idx_request_deltas_message_id`: `message_id` (BTree)
- `idx_request_deltas_removed_reason`: `removed_reason_id WHERE NOT NULL` (BTree partial) — find all compactions

---

### `requests.delta_change_kinds` (vocab)

Baseline lookup. FK target of `requests.deltas.change_kind_id`; the
`request_deltas_reason_only_on_removed` CHECK on `requests.deltas` keys on ids 1 and 2.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Change kind |

Seeded: `added` (1), `removed` (2).

---

### `requests.delta_removed_reasons` (vocab)

Baseline lookup. FK target of `requests.deltas.removed_reason_id`, required on `removed` rows.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Why the message left the conversation |

Seeded: `compaction` (1), `tail_rebuild` (2), `unknown` (3).

---



### `requests.diagnostics`

One row per weasel-pattern analyzer finding. Every field carries the full analyzer-emitted bytes.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `category` | `TEXT NOT NULL` | Weasel category (e.g. `justifying_shortcuts`) |
| `analyzer` | `TEXT NOT NULL` | Analyzer identifier (e.g. `weasel`) |
| `advice` | `TEXT NOT NULL` | Full advice text |
| `matched_text` | `TEXT NOT NULL` | Full matched span |
| `match_range` | `JSONB NULL` | Byte/char range of the match; NULL when analyzer did not pin a span |
| `other_hits` | `JSONB NOT NULL DEFAULT '[]'` | Auxiliary match ranges |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Analyzer timestamp |

**Indexes:**
- `idx_request_diagnostics_request_id`: `request_id` (BTree)
- `idx_request_diagnostics_analyzer_created_at`: `(analyzer, created_at DESC)` (BTree)
- `idx_request_diagnostics_category_created_at`: `(category, created_at DESC)` (BTree)
- `idx_request_diagnostics_created_at`: `created_at DESC` (BTree) — unfiltered time-window scan

---

### `requests.tools`

One row per tool **declared** in `body.tools[]`, preserving declaration order. Semantically distinct from
`tools.uses` (tools invoked by the model, not declared in the request).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `ord` | `INTEGER NOT NULL` | 0-based declaration order |
| `tool_name` | `TEXT NOT NULL` | Full declared tool name, no truncation |
| `definition_id` | `BIGINT NULL` | FK → `tools.definitions(id)` ON DELETE RESTRICT; the content-addressed schema for this declaration. NULL on every row captured before migration `20260721060000` — `mike backfill-tool-definitions` populates it from `request_body`; the live write path always sets it |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_request_tools_request`: `request_id` (BTree)
- `idx_request_tools_name`: `tool_name` (BTree)
- `idx_request_tools_definition_id`: `definition_id` (BTree)

---

### `requests.errors`

One row per request that terminated with `status_id = 4` (error). The full error message text, never
truncated.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `message` | `TEXT NOT NULL` | Full error text |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_request_errors_request_id`: `request_id` (BTree)

---

### `requests.response_intercepts`

Added by `20260921130000_response_intercepts.sql` ([issue #318](https://github.com/temet-ai/mike-private/issues/318)).
Durable evidence that a response-replacing path ran: one row per decision, keyed by session so it
survives a request row that was never persisted (fallback id), and by request when one exists.
Before it, the only proof a weasel intercept fired was a log line. Written by
`PersistHandle::insert_response_intercept` (`store/persistence/response_intercepts.rs`) from the
weasel intercept and proxy-declared-tool paths (`weasel_intercept/`). No REST route, MCP tool or
worker reads it yet; only the e2e test `weasel_intercept_evidence.rs` does.

`outcome`: `weasel_correction` (the streamed answer was discarded and a correction request sent;
`subject` is the forbidden word), `proxy_tool_answer` (the proxy answered a proxy-declared tool
call; `subject` is the tool name), `not_scanned_buffered` (intercept armed but the upstream
response was not an event stream, so no scan ran; `subject` NULL).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request whose response was replaced or left unscanned, NULL when it was never persisted or has been deleted |
| `outcome` | `TEXT NOT NULL` | CHECK `IN ('weasel_correction', 'proxy_tool_answer', 'not_scanned_buffered')` |
| `subject` | `TEXT NULL` | The forbidden word or tool name; NULL for `not_scanned_buffered` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_response_intercepts_session_created`: `(session_id, created_at DESC)` (BTree)
- `idx_response_intercepts_request`: `request_id WHERE request_id IS NOT NULL` (BTree partial)
- `brin_response_intercepts_created_at`: `created_at` (BRIN)

---


### `requests.system_prompts`

One row per request that carried a classifiable system prompt. `override_id` is a plain bigint; its FK to
`overrides.overrides(id)` is added in the trailing ALTER block (the overrides schema is created after the
requests block).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `prompt_sha256` | `TEXT NULL` | Fingerprint for dedup and debugging; not indexed, never a join key |
| `override_id` | `BIGINT NULL` | Plain bigint; FK added via ALTER → `overrides.overrides(id)` |
| `system_prompt_id` | `BIGINT NULL` | FK → `system_prompts.system_prompts(id)`; set by the capture write from the registry insert's `RETURNING id`; the join key for every reader. NULL when the request carried no classified prompt |
| `override_applied` | `BOOLEAN NOT NULL DEFAULT FALSE` | Whether the override was applied |
| `unknown_blocks` | `JSONB NULL` | Structural-mismatch payload when shape couldn't be classified |
| `filter_modified` | `BOOLEAN NOT NULL DEFAULT FALSE` | Whether the filter pipeline modified the prompt |

**Indexes:**
- `idx_request_system_prompts_override_id`: `override_id` (BTree)
- `idx_request_system_prompts_sysprompt_id`: `system_prompt_id` (BTree)

---

### `requests.system_prompt_firing_rules`

Ordered FK junction table recording which `rules.rules` fired (in fire order) for a request's system-prompt
observation. Replaces the old `firing_rule_ids uuid[]` array column.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_system_prompt_id` | `BIGINT NOT NULL` | FK → `requests.system_prompts(id)` ON DELETE CASCADE |
| `filter_rule_id` | `BIGINT NOT NULL` | FK → `rules.rules(id)` ON DELETE RESTRICT |
| `ord` | `INTEGER NOT NULL` | 0-based fire order (CHECK >= 0) |

**Indexes:**
- `idx_rsp_firing_rules_filter_rule_id`: `filter_rule_id` (BTree)

---

### `requests.request_bodies_v`

Added in migration `20260610000004_capture_inspection_views.sql`, one of four capture-inspection views
(the sibling `requests.requests_v` is not documented here — it carries no columns beyond
`requests.requests` with its lookup FKs resolved to codes). Decodes the captured wire body to `jsonb` and
materializes the full system-prompt text per row. Only rows with a non-NULL `request_body` appear (nothing
to decode otherwise). Every value is full-content, never sliced, per
`.claude/rules/agent-behavior-and-data.md`.

Columns: `request_id`, `model`, `created_at`, `body` (full wire payload as jsonb), `is_subagent` (billing
marker `cc_is_subagent=true` found anywhere in the raw body text), `system_text` (full text of all system
blocks, `\n\n`-joined in wire order, or the bare string when the API shape is a plain string), `system_len`
(char length of `system_text`), `system_block_count`, `persona_text` (full text of system block index 2,
the persona block; NULL when the system array has fewer than 3 blocks).

`tools.uses_v` (below) LEFT JOINs this view for `is_subagent`/`persona_text`, reading NULL — honest
"unknown", not false — when the owning request carries no captured body.

---

## Domain: `responses`

### Vocabulary tables

- `responses.statuses`: HTTP status codes 100–599 (PK = status code; FK target for `responses.responses.response_status`)
- `responses.stop_reasons`: `end_turn`(1) `max_tokens`(2) `stop_sequence`(3) `tool_use`(4) `pause_turn`(5) `error`(6) `refusal`(7)
- `responses.stream_block_types`: `text`(1) `thinking`(2) `tool_use`(3) (FK target for `responses.stream_blocks.block_type_id`)
- `responses.stream_delta_types`: `text`(1) `thinking`(2) `input_json`(3)

### `responses.responses`

One row per terminal request — absent means in-flight or error-before-response. Write-once at
`snapshot_terminal` time. Four `GENERATED ALWAYS AS STORED` columns hoist typed token counts out of
the `token_usage` JSONB at write time, so per-turn sums are `SUM(col)` instead of read-time JSONB parsing.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `response_status` | `SMALLINT NOT NULL` | FK → `responses.statuses(id)` (HTTP status code) |
| `response_headers` | `JSONB NOT NULL DEFAULT '[]'` | Response headers |
| `response_body` | `BYTEA NOT NULL DEFAULT '\x'` | Body bytes; empty for streaming |
| `upstream_request_id` | `TEXT NULL` | Anthropic `request-id` response header |
| `upstream_message_id` | `TEXT NULL` | SSE `message_start.message.id` (streaming only) |
| `token_usage` | `JSONB NOT NULL DEFAULT '{}'` | Full token accounting JSONB; source of truth |
| `input_tokens` | `BIGINT GENERATED STORED` | Hoisted from `token_usage->>'input_tokens'` |
| `output_tokens` | `BIGINT GENERATED STORED` | Hoisted from `token_usage->>'output_tokens'` |
| `cache_creation_input_tokens` | `BIGINT GENERATED STORED` | Hoisted from `token_usage->>'cache_creation_input_tokens'` |
| `cache_read_input_tokens` | `BIGINT GENERATED STORED` | Hoisted from `token_usage->>'cache_read_input_tokens'` |
| `stop_reason_id` | `BIGINT NULL` | FK → `responses.stop_reasons(id)` |
| `stop_details` | `JSONB NULL` | Refusal-category object when `stop_reason='refusal'`; NULL otherwise |
| `completed_at` | `TIMESTAMPTZ NOT NULL` | Stream termination wall clock |
| `persisted_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When row was written |

**Indexes:**
- `idx_request_responses_request_id`: `request_id` (BTree)
- `idx_request_responses_completed_at`: `completed_at` BRIN
- `idx_request_responses_stop_reason`: `stop_reason_id WHERE NOT NULL` (BTree partial)
- `idx_request_responses_upstream_request_id`: `upstream_request_id WHERE NOT NULL` (BTree partial)

---

### `responses.statuses` (vocab)

Baseline lookup of HTTP status codes; the primary key is the code itself. FK target of
`responses.responses.response_status`. `20260721030000_missing_fk_indexes.sql` deliberately leaves
that FK unindexed: the 71 rows are never deleted.

| Column | Type | Purpose |
|---|---|---|
| `id` | `SMALLINT` | Primary key; the status code |
| `reason` | `TEXT NOT NULL` | Reason phrase |

**Constraints:**
- `response_statuses_code_range`: `id >= 100 AND id <= 599`

Seeded (71 rows): 100 Continue, 101 Switching Protocols, 102 Processing, 103 Early Hints, 200 OK,
201 Created, 202 Accepted, 203 Non-Authoritative Information, 204 No Content, 205 Reset Content,
206 Partial Content, 207 Multi-Status, 208 Already Reported, 226 IM Used, 300 Multiple Choices,
301 Moved Permanently, 302 Found, 303 See Other, 304 Not Modified, 305 Use Proxy,
307 Temporary Redirect, 308 Permanent Redirect, 400 Bad Request, 401 Unauthorized,
402 Payment Required, 403 Forbidden, 404 Not Found, 405 Method Not Allowed, 406 Not Acceptable,
407 Proxy Authentication Required, 408 Request Timeout, 409 Conflict, 410 Gone,
411 Length Required, 412 Precondition Failed, 413 Content Too Large, 414 URI Too Long,
415 Unsupported Media Type, 416 Range Not Satisfiable, 417 Expectation Failed, 418 I'm a teapot,
421 Misdirected Request, 422 Unprocessable Content, 423 Locked, 424 Failed Dependency,
425 Too Early, 426 Upgrade Required, 428 Precondition Required, 429 Too Many Requests,
431 Request Header Fields Too Large, 451 Unavailable For Legal Reasons,
500 Internal Server Error, 501 Not Implemented, 502 Bad Gateway, 503 Service Unavailable,
504 Gateway Timeout, 505 HTTP Version Not Supported, 506 Variant Also Negotiates,
507 Insufficient Storage, 508 Loop Detected, 510 Not Extended,
511 Network Authentication Required, 520 Web Server Returned an Unknown Error,
521 Web Server Is Down, 522 Connection Timed Out, 523 Origin Is Unreachable,
524 A Timeout Occurred, 525 SSL Handshake Failed, 526 Invalid SSL Certificate,
527 Railgun Error, 529 Site Overloaded.

---

### `responses.stop_reasons` (vocab)

Baseline lookup of the API's `stop_reason` values. FK target of `responses.responses.stop_reason_id`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Stop reason |

Seeded: `end_turn` (1), `max_tokens` (2), `stop_sequence` (3), `tool_use` (4), `pause_turn` (5),
`error` (6), `refusal` (7).

---



### `responses.errors`

One row per response-phase error (upstream or mid-stream failure). Full error text, never truncated. Mirrors
`requests.errors`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `response_id` | `BIGINT NOT NULL` | FK → `responses.responses(id)` ON DELETE CASCADE |
| `message` | `TEXT NOT NULL` | Full error text |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_response_errors_response_id`: `response_id` (BTree)

---

### `responses.stream_blocks`

Added in migration `20260625000007_stream_blocks.sql`, superseding `responses.stream_chunks` (dropped in the
same migration; the verbatim per-delta record was discarded, operator-approved). One row per **assembled
content block** rather than per raw SSE delta: consecutive same-type deltas are coalesced into a single block
row at capture time. `content` carries the full block text — the text for `text` blocks, the thinking text
for `thinking` blocks, empty string for `tool_use` blocks (whose data lives in `tools.uses`, joined via
`wire_tool_use_id`). Reconstructed via `STRING_AGG(content ORDER BY request_id, seq_ord)`, mirroring the
old per-delta reconstruction.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `block_type_id` | `BIGINT NOT NULL` | FK → `responses.stream_block_types(id)`: `text`(1) \| `thinking`(2) \| `tool_use`(3) |
| `seq_ord` | `INTEGER NOT NULL` | 0-based block order within the request (CHECK >= 0) |
| `content` | `TEXT NOT NULL` | Full block text; never truncated. Empty string for `tool_use` blocks |
| `signature` | `TEXT NULL` | Thinking-block signature, captured going forward; NULL for text/tool_use blocks and for historical thinking blocks migrated from the per-delta record |
| `wire_tool_use_id` | `TEXT NULL` | For `tool_use` blocks: the provider's verbatim tool_use id, joined against `tools.uses.wire_tool_use_id` for name/input. NULL otherwise (`stream_blocks_wire_id_only_tool_use` CHECK) |
| `content_sha256` | `TEXT NOT NULL` | SHA-256 fingerprint of `content` for integrity/display only; never indexed, never joined/matched |
| `arrived_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Block completion time |
| `byte_size` | `INTEGER NOT NULL` | Byte length of `content` (CHECK >= 0) |

**Constraints:**
- `stream_blocks_byte_size_nonneg`: CHECK `byte_size >= 0`
- `stream_blocks_seq_ord_nonneg`: CHECK `seq_ord >= 0`
- `stream_blocks_content_sha256_is_64_hex`: CHECK `content_sha256 ~ '^[0-9a-f]{64}$'`
- `stream_blocks_wire_id_only_tool_use`: CHECK `block_type_id = 3 OR wire_tool_use_id IS NULL`
- `uq_stream_blocks_request_seq`: UNIQUE `(request_id, seq_ord)` — write-once idempotency, no sha needed

**Indexes:**
- `idx_response_stream_blocks_request_id`: `request_id` (BTree)
- `idx_response_stream_blocks_session_id`: `session_id` (BTree)
- `idx_response_stream_blocks_block_type_id`: `block_type_id` (BTree)
- `idx_response_stream_blocks_request_type_seq`: `(request_id, block_type_id, seq_ord)` (BTree) — covering index for the per-request, type-filtered, ordered block read
- `idx_response_stream_blocks_arrived_at`: `arrived_at` BRIN

---

### `responses.stream_block_types` (vocab)

Added by `20260625000007_stream_blocks.sql`. Closed lookup of assembled content-block kinds for
`responses.stream_blocks.block_type_id`; mirrors the CHECK/FK pattern of the legacy
`responses.stream_delta_types`, whose `input_json` delta kind folds into the `tool_use` block kind.
Joined by name wherever a turn's text is assembled (`store/persistence/assemble/`,
`turn_facts/text_facts.sql`, `turn_lifecycle/materialize.sql`, `steering/guard/replay_turns.sql`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Block kind |

**Constraints:**
- `stream_block_types_code_known`: `code IN ('text', 'thinking', 'tool_use')`

Seeded: `text` (1), `thinking` (2), `tool_use` (3).

---


## Domain: `tools`

### `tools.definitions`

Content-addressed tool-definition store — mirrors `messages.messages` + `requests.messages`: one row per
DISTINCT `tools[]` entry, stored EXACTLY as received on the wire (key order, formatting, and
`cache_control` if present all preserved — no canonicalisation), bridged from
`requests.tools.definition_id`. Exists so a tool's JSON schema survives even if `request_body` is ever
dropped — `requests.tools` alone records only the declared name.

No canonicalisation is deliberate: measured (500 sampled requests, 37,029 tool declarations, 2026-07-21)
that hashing the raw wire bytes and hashing a sorted-keys/`cache_control`-stripped form produce the
IDENTICAL 682 distinct definitions at the identical 1.84 MB — normalising bought zero extra dedup and would
have cost byte-exact reconstruction of `request_body`'s `tools[]`. `cache_control` itself appeared on 0 of
the sampled declarations. Full reasoning: `crates/mike/src/store/persistence/tools.rs`'s module doc
comment.

`body` is `BYTEA`, not `JSONB` — `jsonb` normalises key order and whitespace on ingest (verified live:
`'{"role":"user","zz":1,"aa":2}'::jsonb` reads back as `{"aa": 2, "zz": 1, "role": "user"}`), so a `jsonb`
column cannot hold a wire form regardless of what bytes are sent to it. Query with
`convert_from(body, 'UTF8')::jsonb` when JSON access is wanted; the stored bytes stay authoritative.

`definition_sha256` carries NO index or UNIQUE constraint (`.claude/rules/rust-database-reliability.md`'s hard ban on
indexing sha columns). Unlike `messages.messages` (millions of rows), this table stays small — 1,167
distinct definitions measured across 87,904 declared-tool rows in a 1% sample — so the write path
(`tools::write_request_tools`, `crates/mike/src/store/persistence/tools.rs`) dedups with a plain `WHERE
definition_sha256 = ANY($1)` scan of the whole table instead. Dedup is GLOBAL across every session (not
scoped to one session lineage the way messages are), since the same builtin tool definitions repeat across
unrelated sessions.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `definition_sha256` | `TEXT NOT NULL` | Lowercase-hex SHA-256 of the tool's own wire bytes, unmodified — content-addressed key, deliberately unindexed |
| `tool_name` | `TEXT NOT NULL` | Declared tool name at first capture |
| `body` | `BYTEA NOT NULL` | Full tool definition exactly as received on the wire (description, input_schema, …) — not `jsonb`, see above |
| `byte_size` | `INTEGER NOT NULL` | Wire byte length (CHECK >= 0) |
| `first_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When this definition was first captured |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Refreshed whenever `write_request_tools` matches this definition again |

**Indexes:**
- `idx_tools_definitions_last_seen`: `last_seen DESC` (BTree)
- `idx_tools_definitions_tool_name`: `tool_name` (BTree)

---

### `tools.uses`

One row per tool-use block **emitted by the model** (write-as-you-go from the response SSE stream).
`wire_tool_use_id` (the provider-assigned `toolu_...` identifier) is the UNIQUE natural key; `id` is the
surrogate PK and the FK target for children (`tools.results`, `agents.dispatches`).

`file_path` and `operation` are resolved at capture time by the Rust tool_name→path-field registry; they
are NOT generated columns or triggers. This keeps the path extraction logic evolvable without a migration.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `wire_tool_use_id` | `TEXT NOT NULL UNIQUE` | Provider-assigned tool_use id (e.g. `toolu_...`) |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE — denormalized from the carrying request for session-scoped scans without a join |
| `ord` | `INTEGER NOT NULL` | 0-based call order within the request |
| `tool_name` | `TEXT NOT NULL` | Tool name verbatim (e.g. `Bash`, `Read`, `Write`, `Agent`) |
| `input` | `JSONB NULL` | Full wire input payload; NULL when model emitted empty input |
| `file_path` | `TEXT NULL` | Touched path, resolved at capture time; NULL for non-path-bearing tools |
| `operation` | `TEXT NULL` | Operation label (e.g. `read`/`write`/`edit`), resolved at capture time |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Capture timestamp |

**Constraints:**
- `uq_tool_uses_wire_tool_use_id`: UNIQUE(`wire_tool_use_id`)
- `uq_tool_uses_request_ord`: UNIQUE(`request_id, ord`)

**Indexes:**
- `idx_tool_uses_request_id`: `request_id` (BTree)
- `idx_tool_uses_session_created_ord`: `(session_id, created_at, ord)` (BTree) — session-scoped, `created_at`-ordered tool-use reads (`list_session_tool_uses`); the composite's prefix makes a standalone `session_id` index redundant, so none exists
- `idx_tool_uses_name`: `tool_name` (BTree)
- `idx_tool_uses_file_path`: `file_path WHERE NOT NULL` (BTree partial) — file-touch queries
- `idx_tool_uses_bash_command_trgm`: GIN trgm on `(input->>'command') WHERE tool_name='Bash'` — command search

---

### `tools.results`

One row per `tool_result` block in a follow-up request. `tool_use_id` is resolved at write time by joining
the wire id against `tools.uses.wire_tool_use_id`; a result whose tool_use was never captured is silently
dropped. `content` carries the full tool result — string or array of typed blocks — never truncated.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `tool_use_id` | `BIGINT NOT NULL` | FK → `tools.uses(id)` ON DELETE CASCADE |
| `ord` | `INTEGER NOT NULL` | 0-based order within the request |
| `is_error` | `BOOLEAN NOT NULL` | Whether the tool returned an error |
| `content` | `JSONB NOT NULL` | Full tool result content (string or typed-block array) |
| `content_sha256` | `TEXT NOT NULL` | 64-char hex SHA-256 of content bytes |
| `content_bytes` | `BIGINT NOT NULL` | Byte length of content |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_tool_results_request_id`: `request_id` (BTree)
- `idx_tool_results_errors`: `request_id WHERE is_error` (BTree partial)
- `idx_tool_results_tool_use_id`: `tool_use_id` (BTree)

---

### `tools.uses_v`

Added in migration `20260610000004_capture_inspection_views.sql`. `tools.uses` with request/session/project
context resolved, so ad-hoc inspection queries stop re-deriving the same joins by hand.

Columns: `id`, `wire_tool_use_id`, `request_id`, `ord`, `tool_name`, `input`, `command` (`input->>'command'`),
`file_path`, `operation`, `created_at`, `request_created_at`, `session_uuid`, `project_path`, `is_subagent`,
`persona_text`.

`is_subagent`/`persona_text` are sourced via LEFT JOIN to `requests.request_bodies_v` — NULL when the owning
request carries no captured body (honest "unknown", not false), rather than a JOIN that silently drops the
tool-use row.

---

## Domain: `agents`

### `agents.sources` (vocab)

Seeded: `observed` (1), `file_backed` (2), `dynamic` (3), `builtin` (4).

### `agents.agents`

One row per named agent persona. `id` is the bigint PK and the addressing key used by persistence and the API;
`uuid` is stored data, not an addressing key or a stable cross-database identity for migration-seeded agents.
Resync resolves agents by `LOWER(name)` and only carries `uuid` through when it inserts a genuinely new row.
Migration `20260722010000_consolidate_agent_aliases.sql` makes this table the sole owner of agent identity:
full persona strings live in `persona_aliases`, while alternate name spellings live in `name_variants`.
It rolls the former `agents.aliases` rows into the matching array by kind, preserves any value from the
legacy singular `alias` column as a name variant, drops that child table, and removes the unused `alias`,
`frontmatter*`, and `skills` columns.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stored UUID copied by resync for genuinely new rows; not used to address agents |
| `name` | `TEXT NOT NULL` | Agent name |
| `persona_aliases` | `TEXT[] NOT NULL DEFAULT '{}'::text[]` | Full persona strings that identify this canonical agent; store resolution is case-insensitive |
| `name_variants` | `TEXT[] NOT NULL DEFAULT '{}'::text[]` | Alternate names for this canonical agent, kept distinct from free-text persona strings; store resolution is case-insensitive |
| `definition_path` | `TEXT NULL` | Where the agent is defined on disk |
| `description` | `TEXT NULL` | Operator-facing notes |
| `source_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `agents.sources(id)` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Constraints:**
- `agents_persona_aliases_no_null`: CHECK `array_position(persona_aliases, NULL) IS NULL`
- `agents_name_variants_no_null`: CHECK `array_position(name_variants, NULL) IS NULL`

**Indexes:**
- `idx_agents_source`: `source_id` (BTree)
- `idx_agents_name`: `name` (BTree)

Both arrays must exist and may be empty; their checks prohibit NULL array elements. The persistence layer can
flatten them for existing API consumers as `{alias, agent_id, kind}` rows, assigning `kind='persona'` to
`persona_aliases` elements and `kind='name_variant'` to `name_variants` elements. The API can therefore preserve
alias kind without restoring a secondary aliases table.

---

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `alias` |  |
| `frontmatter` |  |
| `frontmatter_name` |  |
| `frontmatter_description` |  |
| `frontmatter_model` |  |
| `frontmatter_skills` |  |
| `skills` |  |

### `agents.dispatches`

One row per `Agent` tool-use. A typed extension of `tools.uses` for the Agent tool family. `ON DELETE RESTRICT`
on `agent_id` prevents retiring an agent from silently deleting dispatch history.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `tool_use_id` | `BIGINT NOT NULL` | FK → `tools.uses(id)` ON DELETE CASCADE |
| `agent_id` | `BIGINT NOT NULL` | FK → `agents.agents(id)` ON DELETE RESTRICT |

**Indexes:**
- `idx_agent_dispatches_agent_id`: `agent_id` (BTree)

---

### `agents.instances`

Tracks the lifecycle of each spawned sub-agent. One row is created when a dispatched agent's first request
arrives. `first_request_id` is a real FK ON DELETE SET NULL (not a soft reference).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `dispatch_id` | `BIGINT NOT NULL` | FK → `agents.dispatches(id)` ON DELETE CASCADE |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL |
| `subagent_persona` | `TEXT NULL` | Persona string from the first request |
| `first_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL |
| `started_at` | `TIMESTAMPTZ NULL` | Timestamp of the first request |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_agent_instances_session_id`: `session_id` (BTree)
- `idx_agent_instances_agent_id`: `agent_id` (BTree)
- `agent_instances_session_persona_idx`: `(session_id, subagent_persona) WHERE NOT NULL` (BTree partial)

---

### `agents.work_items`

Added by `20260923130000_work_items.sql`. One row per background job a session launched through
Claude Code — a `run_in_background` Bash command or an async Agent — recognised on the wire from the
launching tool result (`work_items/parse.rs`: the "Command running in background with ID:" and
"Async agent launched successfully." texts) and closed by the `<task-notification>` block a later
request carries. Written inside the request-capture transaction by
`store/persistence/work_items.rs` (`record_from_request_guarded`, which also applies earlier
notifications a request body still carries for items whose finish was missed); the work-registry
worker closes a still-running job from the last line of its output file
(`work_items_reconcile.rs::close_job_from_its_output`), and
`POST /api/sessions/{id}/work-items/backfill` replays a session's requests. Read by the work-registry worker's ledger (`work_registry_worker.rs`,
`work_items/ledger.rs`, `work_items_ledger_rows.sql`), the dossier (`dossier_entries/backing_items.sql`,
`promise_close.sql`), steering facts (`steering_facts/backing.sql`), turn facts
(`turn_facts/checks_load.sql`) and the fleet (`fleet/background_ends.sql`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `wire_tool_use_id` | `TEXT NOT NULL` | The `tool_use` id of the launching call; UNIQUE |
| `kind` | `TEXT NOT NULL` | CHECK `IN ('bash', 'agent')` |
| `task_id` | `TEXT NOT NULL` | Claude Code's id for the job (the background command id or the agent id) |
| `output_path` | `TEXT NULL` | The file the job's output is written to, from the launch text |
| `owner_agent_id` | `TEXT NULL` | `x-claude-code-agent-id` of the sub-agent whose request launched it; NULL for the main thread |
| `description` | `TEXT NULL` | |
| `launched_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request that carried the launch result |
| `launched_at` | `TIMESTAMPTZ NOT NULL` | |
| `state` | `TEXT NOT NULL DEFAULT 'running'` | CHECK `IN ('running', 'completed', 'failed', 'killed', 'unknown')`; `completed` with a non-zero exit code is stored as `failed` |
| `finished_at` | `TIMESTAMPTZ NULL` | |
| `finished_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request that carried the task notification |
| `notified_agent_id` | `TEXT NULL` | `x-claude-code-agent-id` of the request that carried the notification |
| `exit_code` | `INTEGER NULL` | From the notification |
| `summary` | `TEXT NULL` | From the notification |

**Constraints:**
- `uq_work_items_wire_tool_use_id`: UNIQUE `(wire_tool_use_id)`

**Indexes:**
- `idx_work_items_session_launched`: `(session_id, launched_at)` (BTree)
- `idx_work_items_task_id`: `task_id` (BTree)
- `idx_work_items_running`: `session_id WHERE state = 'running'` (BTree partial)

---

### `agents.work_item_attention`

Added by `20260923200000_work_item_attention.sql`. A raise-once ledger of the conditions the
work-registry worker finds in a session's work ledger (`work_items/attention.rs::assess`): kinds
`result_lost`, `agent_silent`, `landed_polling`, `stalled_wait` and `main_idle_with_open_work`. One
row per `(session, kind, subject)`, upserted by `work_item_attention_record.sql`; the worker
(`work_registry_worker/delivery.rs`) mails the session once through the mailbox, retrying with
exponential backoff up to six attempts (`20261007100100_work_item_attention_delivery_attempts.sql`),
and settles the row with a disposition (`20261007100300_work_item_attention_disposition.sql`):
`delivered` once mailed, `stale` when older than the delivery window, `resolved` when no longer
true on the rescan before delivery, `given_up` when the attempts run out. `eval.alarms` copies the
same raise-once idea.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `kind` | `TEXT NOT NULL` | Attention kind (open set, see above) |
| `subject` | `TEXT NOT NULL` | The work item or agent the finding is about |
| `since` | `TIMESTAMPTZ NOT NULL` | When the condition began |
| `message` | `TEXT NOT NULL` | The text mailed to the session |
| `first_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `delivered_message_uuid` | `UUID NULL` | `mailbox.messages.message_uuid` of the delivery (soft reference, no FK) |
| `delivery_attempts` | `INTEGER NOT NULL DEFAULT 0` | Mail attempts so far |
| `last_delivery_attempt_at` | `TIMESTAMPTZ NULL` | |
| `disposition` | `TEXT NOT NULL DEFAULT 'pending'` | CHECK `IN ('pending', 'delivered', 'stale', 'resolved', 'given_up')` |
| `disposition_reason` | `TEXT NULL` | `mailed`, or why the row was settled without delivery |
| `disposition_at` | `TIMESTAMPTZ NULL` | |

**Constraints:**
- `uq_work_item_attention`: UNIQUE `(session_id, kind, subject)`

**Indexes:**
- `idx_work_item_attention_session_seen`: `(session_id, first_seen_at)` (BTree)

---



## Domain: `overrides`

`system_prompt_id` is the join to captured requests, and it holds the lowest registry id for a
prompt's sha. Migration `20261103140000_overrides_point_at_canonical_system_prompt.sql` repaired
older rows that pointed at a later duplicate.

### `overrides.modes` (vocab)

Seeded: `observe` (1), `template` (2), `replace` (3).

### `overrides.sources` (vocab)

Seeded: `manual` (1), `filter` (2).

### `overrides.overrides`

Operator-authored rewrites of captured system prompts (was `prompt_compositions`). `uuid` is the stable
surrogate for deep-links; `mode_id` and `source_id` are FK lookups.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate for deep-links |
| `prompt_sha256` | `TEXT NOT NULL` | Content-address matching `system_prompts.system_prompts.prompt_sha256` |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL |
| `filter_rule_id` | `BIGINT NULL` | FK → `rules.rules(id)` ON DELETE CASCADE; populated for `source='filter'` rows |
| `system_prompt_id` | `BIGINT NOT NULL` | FK → `system_prompts.system_prompts(id)` |
| `agent_name` | `TEXT NULL` | Denormalized cache of `agents.agents.name` |
| `agent_definition_path` | `TEXT NULL` | Where the agent is defined on disk |
| `label` | `TEXT NOT NULL` | Operator-facing label |
| `blocks` | `JSONB NOT NULL` | Operator-authored block array |
| `mode_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `overrides.modes(id)` |
| `notes` | `TEXT NULL` | Free-form operator notes |
| `version` | `BIGINT NOT NULL DEFAULT 1` | Monotonic per-row version counter; bumped on PATCH |
| `source_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `overrides.sources(id)` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_overrides_agent_id`: `agent_id` (BTree)
- `idx_overrides_filter_rule_id`: `filter_rule_id` (BTree)
- `idx_overrides_sysprompt_id`: `system_prompt_id` (BTree)
- `idx_overrides_agent_name`: `agent_name WHERE NOT NULL` (BTree partial)
- `idx_overrides_prompt_sha256`: `prompt_sha256` (BTree)

**Trigger:** `trg_prompt_compositions_row_changed` — on `overrides.overrides`; NOTIFY `proxy.row_changed`

---

### `overrides.revisions`

History table for `overrides.overrides`. Every successful INSERT or PATCH writes a snapshot row here.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `override_id` | `BIGINT NOT NULL` | FK → `overrides.overrides(id)` ON DELETE CASCADE |
| `version` | `BIGINT NOT NULL` | Version counter |
| `label` | `TEXT NOT NULL` | Snapshot of label at this version |
| `blocks` | `JSONB NOT NULL` | Snapshot of blocks at this version |
| `mode_id` | `BIGINT NOT NULL` | FK → `overrides.modes(id)` |
| `notes` | `TEXT NULL` | Snapshot of notes |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_override_revisions_override_id`: `override_id` (BTree)

---

## Domain: `memory`

### Vocabulary tables

| Table | Seeded values |
|---|---|
| `memory.turn_outcomes` | `unknown`(1) `accepted`(2) `corrected`(3) `abandoned`(4) `shipped`(5) |
| `memory.chunk_roles` | `prompt`(1) `assistant`(2) |
| `memory.lesson_kinds` | `correction`(1) `convention`(2) `warning`(3) `preference`(4) |
| `memory.lesson_sources` | `agent`(1) `operator`(2) `copilot`(3) `harvester`(4) `directive`(5) `judge`(6) `carry`(7) |
| `memory.lesson_stages` | `raw`(1) `daily`(2) `long_term`(3) `archived`(4) |
| `memory.lesson_archived_reasons` | `duplicate`(1) `expired`(2) `trimmed`(3) `superseded`(4) `operator`(5) `rejected_by_operator`(6) `not_a_lesson`(7) |
| `memory.lesson_relation_kinds` | `supersedes`(1) `refines`(2) `recurs`(3) `contradicts`(4) |
| `memory.lesson_relation_methods` | `declared`(1) `artifact_reference`(2) `adjudicated`(3) `operator`(4) `write_time`(5) `rewritten`(6) |
| `memory.lesson_relation_statuses` | `proposed`(1) `confirmed`(2) `rejected`(3) `contested`(4) |
| `memory.consolidation_run_kinds` | `session`(1) `nightly`(2) `gc`(3) |
| `memory.consolidation_triggers` | `schedule`(1) `startup_catchup`(2) `manual`(3) |
| `memory.consolidation_run_statuses` | `running`(1) `completed`(2) `partial`(3) `failed`(4) |
| `memory.consolidation_action_kinds` | `promoted_to_daily`(1) `promoted_to_long_term`(2) `archived_duplicate`(3) `archived_trimmed`(4) `archived_expired`(5) `deleted_expired`(6) `evaluated`(7) |
| `memory.consolidation_outcomes` | `applied`(1) `failed`(2) `skipped`(3) |
| `memory.consolidation_decided_by` | `embedding_distance`(1) `rank_order`(2) `expiry`(3) |
| `memory.lesson_injection_lanes` | `primary`(1) `subagent`(2) |

### `memory.operator_turns`

One row per Claude Code user prompt, spanning everything the model did between that prompt and the next
operator input.

**Two-state lifecycle:** `OPEN` (`closed_at IS NULL`) → `CLOSED` (`closed_at IS NOT NULL`).

**Aggregate cache columns** (`tool_call_count` … `final_assistant_text`) are materialized **once at
turn-close** by the Rust turn-classifier, computed in one pass from the normalized tables. They are NULL
while the turn is open. `operator_turn_aggregates_v` reads these cached columns for closed turns and
recomputes live only for the one open turn.

`outcome_id` is a FK lookup; old `outcome TEXT` + CHECK replaced.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `start_request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `end_request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `outcome_evidence_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL |
| `outcome_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `memory.turn_outcomes(id)` |
| `classified_at` | `TIMESTAMPTZ NULL` | When outcome was assigned |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `closed_at` | `TIMESTAMPTZ NULL` | NULL = OPEN; NOT NULL = CLOSED; write-once after close |
| `embedded_at` | `TIMESTAMPTZ NULL` | Set by embed worker when chunks have been written |
| `tool_call_count` | `BIGINT NULL` | Materialized at close; NULL while open |
| `tool_error_count` | `BIGINT NULL` | Materialized at close; NULL while open |
| `weasel_findings_count` | `BIGINT NULL` | Materialized at close; NULL while open |
| `total_input_tokens` | `BIGINT NULL` | Materialized at close; NULL while open |
| `total_output_tokens` | `BIGINT NULL` | Materialized at close; NULL while open |
| `tools_used` | `TEXT[] NULL` | Materialized at close; NULL while open |
| `agents_dispatched` | `TEXT[] NULL` | Materialized at close; NULL while open |
| `files_touched` | `TEXT[] NULL` | Materialized at close; NULL while open |
| `final_assistant_text` | `TEXT NULL` | Concatenated assistant text for the turn; materialized at close |
| `user_prompt` | `TEXT NULL` | User prompt that opened this turn; written at turn-open |
| `judge_model` | `TEXT NULL` | OpenRouter model id that produced the judge verdict; NULL until the judge worker processes this turn |
| `judge_rubric_version` | `TEXT NULL` | Rubric version string used by the judge worker; NULL until judged. Used to re-queue turns when the rubric changes |
| `subject_command` | `TEXT NULL` | Slash-command/skill that opened the turn, parsed from the opening `user_prompt`; NULL when the opening prompt is not a command invocation |
| `subject_agent` | `TEXT NULL` | Agent persona active for the turn — the opening request's `subagent_persona`, defaulting to `main` |
| `skills_used` | `TEXT[] NULL` | Distinct skill names invoked mid-turn via the `Skill` tool; materialized at close, mirrors `agents_dispatched` |
| `opened_by` | `TEXT NULL` | CHECK `operator`, `machine`, `agent`. `operator`: a genuine operator prompt opened the row. `machine`: an in-lane machine event (hook feedback, teammate message, system notification, loop wakeup) opened it. `agent`: a historical row opened by a request that carried an agent id, written only by `mike origin-backfill`. NULL on rows created before migration `20260924100000`; readers treat NULL as operator |
| `search_tsv` | `TSVECTOR NULL` | `GENERATED ALWAYS AS (to_tsvector('english', coalesce(user_prompt,'') \|\| ' ' \|\| coalesce(final_assistant_text,''))) STORED`; backs `idx_operator_turns_search_tsv` full-text search |

`judge_model`/`judge_rubric_version` (migration `20260625000001_judge_labeling.sql`) are the judge worker's
reproducibility metadata for the periodic sweep over closed, `unknown`-outcome turns — set atomically with
`outcome_id` in `stamp_turn_judgment` so a partial update cannot leave them inconsistent. Distinct from the
`session_judges` domain below: this pair labels one *turn*; `session_judges.verdicts` scores a whole
*session*.

**Indexes:**
- `idx_operator_turns_project`: `(project_id, created_at DESC)` (BTree)
- `idx_operator_turns_outcome_created`: `(outcome_id, created_at DESC)` (BTree)
- `idx_operator_turns_start_request_id`: `start_request_id` (BTree)
- `idx_operator_turns_end_request_id`: `end_request_id` (BTree)
- `idx_operator_turns_evidence_request_id`: `outcome_evidence_request_id` (BTree)
- `idx_operator_turns_unjudged`: `closed_at WHERE outcome_id = 1 AND closed_at IS NOT NULL` (BTree partial) — judge worker's claim sweep, oldest first
- `idx_operator_turns_session_id`: `(session_id, created_at DESC)` (BTree)
- `idx_operator_turns_subject_command`: `subject_command` (BTree)
- `idx_operator_turns_subject_agent`: `subject_agent` (BTree)
- `idx_operator_turns_embed_queue`: `closed_at WHERE closed_at IS NOT NULL AND embedded_at IS NULL AND (opened_by IS NULL OR opened_by = 'operator')` (BTree partial) — embed queue and its count
- `idx_operator_turns_open`: `session_id WHERE closed_at IS NULL` (BTree partial) — open turns
- `idx_operator_turns_transient_backoff`: `judge_next_attempt_at WHERE judge_failure_reason = 'transient_exhausted'` (BTree partial)
- `operator_turns_judge_queue_idx`: `(closed_at, id) WHERE judge_queued_at IS NOT NULL` (BTree partial) — the live judge claim and backlog count start here; `judge_queued_at` is set by the turn-facts upsert when a turn becomes judge-eligible and cleared when a matching live verdict lands or the turn fails permanently
- `idx_operator_turns_closed_at`: `closed_at WHERE closed_at IS NOT NULL` (BTree partial) — live judge window
- `idx_operator_turns_search_tsv`: `search_tsv` (GIN) — full-text search

---

Migration `20260925000300_turn_outcome_projection.sql` adds the projection columns, all nullable and written only by `project_turn_outcomes`:

| Column | Type | Meaning |
|---|---|---|
| `reaction` | `TEXT` | `corrected`, `redirected`, `accepted`, `no_response`, `unclear` |
| `delivery` | `TEXT` | `shipped`, `delivered`, `partial`, `none`, `question`, `not_applicable` |
| `live_verdict_id` | `BIGINT` FK `memory.turn_verdicts(id)` `ON DELETE SET NULL` | the live verdict the label came from; NULL for an operator label or no verdict |
| `outcome_source` | `TEXT` | `judge`, `operator`, `frustration_close`, `no_verdict`; NULL = never projected (a legacy label, or a turn still waiting) |
| `outcome_projection_version` | `SMALLINT` | the mapping version that last wrote the row |
| `outcome_projected_at` | `TIMESTAMPTZ` | when the projection last changed the row |

### `memory.operator_turn_legacy_labels`

One row per turn whose pre-projection label the projection overwrote: `turn_id` (PK, no FK so retention cannot erase it), `outcome_id`, `classified_at`, `judge_model`, `judge_rubric_version`, `rationale`, `unmet_ask`, `evidence_quality`, `judge_failure_reason`, `snapshot_at`. Written once per turn, in the projection statement, before its update.

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `turn_id` | bigint PRIMARY KEY |
| `outcome_id` | bigint NOT NULL |
| `classified_at` | timestamptz NULL |
| `judge_model` | text NULL |
| `judge_rubric_version` | text NULL |
| `rationale` | text NULL |
| `unmet_ask` | text NULL |
| `evidence_quality` | text NULL |
| `judge_failure_reason` | text NULL |
| `snapshot_at` | timestamptz NOT NULL DEFAULT now() |

### `eval.holdout_split(turn_id bigint, holdout_percent bigint)`

Added in migration `20261103100000_eval_holdout_split_fn.sql`. An immutable SQL function that
answers `holdout` or `dev` for a turn from a stable hash of its id, so a turn stays on one side of
a split between runs.

### `eval.rejudge_runs`

The D9 history re-judge: `bundle_id`, `window_since`, `window_until`, `daily_call_cap`, `status` (`queued`, `running`, `done`, `cancelled`), `estimated_calls`, `estimated_usd`, `created_by`, and timestamps. A unique partial index allows one queued or running run. Progress and cost are read from `memory.turn_verdicts` at request time.

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `role` | text NOT NULL DEFAULT 'turn_judge' CHECK (role IN ('turn_judge')) |
| `bundle_id` | bigint NOT NULL |
| `window_since` | timestamptz NOT NULL |
| `window_until` | timestamptz NOT NULL |
| `daily_call_cap` | integer NOT NULL CHECK (daily_call_cap >= 1) |
| `status` | text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'cancelled')) |
| `estimated_calls` | integer NOT NULL CHECK (estimated_calls >= 0) |
| `estimated_usd` | double precision NULL |
| `created_by` | text NOT NULL |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `started_at` | timestamptz NULL |
| `finished_at` | timestamptz NULL |

### `memory.operator_turn_chunks`

Chunked embedding store for semantic recall. Each turn's text is split into ≤1500-character chunks, each
embedded independently. Recall takes `MIN(chunk_embedding <=> query_vec)` over all chunks of a turn.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `turn_id` | `BIGINT NOT NULL` | FK → `memory.operator_turns(id)` ON DELETE CASCADE |
| `chunk_idx` | `INTEGER NOT NULL` | 0-based ordinal within `(turn_id, chunk_role_id)` |
| `chunk_role_id` | `BIGINT NOT NULL` | FK → `memory.chunk_roles(id)` |
| `chunk_text` | `TEXT NOT NULL` | ≤1500 chars of source text |
| `chunk_embedding` | `vector(768) NOT NULL` | 768-dimension embedding (the configured `embedding.model`) |
| `embedding_model` | `TEXT NOT NULL` | Model identifier |
| `embedded_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_otc_turn`: `turn_id` (BTree)
- `idx_otc_embedding`: `chunk_embedding vector_cosine_ops` HNSW m=16 ef=64 — cosine ANN recall

---

### `memory.chunk_roles`

Lookup for `memory.operator_turn_chunks.chunk_role_id`: which side of the turn a chunk came from. Seeded
by the baseline. Deliberately unindexed as an FK target (`20260721030000_missing_fk_indexes.sql`): two
rows, never deleted.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `prompt`(1) `assistant`(2).

---


### `memory.lessons`

Operator/agent-authored memory entries scoped to a project. Pulled by `recall_project_context`. `kind_id`,
`stage_id`, `source_id`, `archived_reason_id` are FK lookups. `request_id` is a real FK ON DELETE SET NULL.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `project_id` | `BIGINT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `source_turn_id` | `BIGINT NULL` | FK → `memory.operator_turns(id)` ON DELETE SET NULL |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL |
| `agent_instance_id` | `BIGINT NULL` | FK → `agents.instances(id)` ON DELETE SET NULL |
| `duplicate_of_id` | `BIGINT NULL` | FK → `memory.lessons(id)` ON DELETE SET NULL (self-referential) |
| `request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL |
| `kind_id` | `BIGINT NOT NULL` | FK → `memory.lesson_kinds(id)` |
| `text` | `TEXT NOT NULL` | Full lesson text; no length cap, never truncated. A text that fails the rule-shape check is stored whole and flagged in `memory.lesson_shape_findings` |
| `stage_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `memory.lesson_stages(id)` |
| `source_id` | `BIGINT NOT NULL DEFAULT 1` | FK → `memory.lesson_sources(id)` |
| `text_sha256` | `BYTEA NULL` | SHA-256 for dedup |
| `embedding` | `vector(768) NULL` | Embedding for semantic recall (the configured `embedding.model`) |
| `embedding_model` | `TEXT NULL` | |
| `embedded_at` | `TIMESTAMPTZ NULL` | |
| `consolidation_count` | `INTEGER NOT NULL DEFAULT 0` | |
| `last_consolidated_at` | `TIMESTAMPTZ NULL` | |
| `archived_at` | `TIMESTAMPTZ NULL` | |
| `archived_reason_id` | `BIGINT NULL` | FK → `memory.lesson_archived_reasons(id)` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `expires_at` | `TIMESTAMPTZ NULL` | NULL = never expires |
| `topic` | `TEXT NULL` | Optional topic key for verb-grammar recall (`mem:recall`/`mem:update` tags). NULL for lessons written before the recall schema or via the MCP `record_lesson` tool |
| `placement_id` | `SMALLINT NULL` | FK → `memory.lesson_placements(id)`. Where the lesson belongs: decided in the write-time model call, or by the placement pass for lessons written without one. NULL = not placed yet. Only lessons placed as `skill` feed skill drafts |
| `placement_covered_by` | `TEXT NULL` | The memory file or skill, as the proposer was shown it, that already covers the lesson or that it belongs in |
| `placement_reason` | `TEXT NULL` | One sentence from the model saying why |
| `placed_at` | `TIMESTAMPTZ NULL` | When the placement was stored |

**Constraints:**
- `lessons_placement_consistent`: `(placement_id IS NULL) = (placed_at IS NULL)`
- `lessons_archive_reason_consistent`: `(archived_reason_id IS NOT NULL) = (archived_at IS NOT NULL)`
- `lessons_consolidation_count_non_negative`: `consolidation_count >= 0`
- `lessons_duplicate_of_requires_archive`: `(duplicate_of_id IS NULL) OR (archived_reason_id = 1)`
- `lessons_embedding_consistent`: embedding, embedded_at, embedding_model must all be NULL or all non-NULL
- `lessons_stage_archive_consistent`: `(stage_id = 4) = (archived_at IS NOT NULL)`

**Indexes:**
- `idx_lessons_project`: `(project_id, created_at DESC)` (BTree)
- `idx_lessons_session_id`: `session_id` (BTree)
- `idx_lessons_session`: `(session_id, created_at DESC) WHERE NOT NULL` (BTree partial)
- `idx_lessons_agent_id`: `agent_id` (BTree)
- `idx_lessons_duplicate_of_id`: `duplicate_of_id` (BTree)
- `idx_lessons_agent_instance`: `(agent_instance_id, created_at DESC) WHERE NOT NULL` (BTree partial)
- `idx_lessons_daily_by_project`: `(project_id, created_at) WHERE stage_id=2` (BTree partial)
- `idx_lessons_raw_pending`: `(project_id, created_at) WHERE stage_id=1 AND embedded_at IS NULL` (BTree partial)
- `idx_lessons_injectable_global`: `(agent_id, stage_id, created_at DESC) WHERE stage IN (2,3) AND NOT archived AND project IS NULL` (BTree partial)
- `idx_lessons_injectable_project`: `(project_id, agent_id, stage_id, created_at DESC) WHERE stage IN (2,3) AND NOT archived AND project IS NOT NULL` (BTree partial)
- `idx_lessons_embedding`: `embedding vector_cosine_ops WHERE NOT NULL` HNSW m=16 ef=64 — semantic lesson recall
- `idx_lessons_topic`: `(project_id, topic) WHERE topic IS NOT NULL AND archived_at IS NULL` (BTree partial) — exact-match topic recall

---

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `portable_origin` | jsonb NOT NULL DEFAULT '{}' |

### `memory.lesson_relations`

Added in `20260729020000_memory_lesson_relations.sql`. Directional "B updates A" graph between lessons
(`from_lesson_id` is the newer lesson, `to_lesson_id` the older one it supersedes, refines, recurs or
contradicts). Never a deletion mechanism: a row only records that a relationship was proposed and how it
was decided; archiving stays with `memory.consolidation_actions`. `candidate_distance` only nominated the
pair for comparison; direction and kind come from `evidence_json`, the rationales and adjudication. Written
by `crates/mike/src/lesson_relations/` (detection, rule-based and adjudicated pairs; the `lesson_relations`
`llm.calls` role) through `store/persistence/lesson_relations.rs`, and at lesson write time by
`store/persistence/lessons/write_time.rs` (`POST /api/lessons`, methods `write_time` and `rewritten`).
Operator decisions arrive through `POST /api/memory/relations/{id}/confirm` and `/reject`; reads are
`GET /api/memory/relations` and `GET /api/lessons/{id}/relations`; the nightly pass
(`lesson_worker/nightly.rs`, `apply_confirmed_lesson_relations`) acts on confirmed rows. Lesson files
import relations through `project_knowledge/lesson_relations_import.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `from_lesson_id` | `BIGINT NOT NULL` | FK → `memory.lessons(id)` (NO ACTION). The newer, superseding/refining lesson |
| `to_lesson_id` | `BIGINT NOT NULL` | FK → `memory.lessons(id)` (NO ACTION). The older lesson being updated |
| `kind_id` | `BIGINT NOT NULL` | FK → `memory.lesson_relation_kinds(id)` |
| `method_id` | `BIGINT NOT NULL` | FK → `memory.lesson_relation_methods(id)` |
| `status_id` | `BIGINT NOT NULL` | FK → `memory.lesson_relation_statuses(id)` |
| `candidate_distance` | `DOUBLE PRECISION NULL` | Cosine distance that nominated the pair; never decides kind, direction or status |
| `evidence_json` | `JSONB NOT NULL DEFAULT '{}'` | Verifiable claims only: quoted spans, artifact refs, timestamps |
| `proposer_model` | `TEXT NULL` | |
| `proposer_rationale` | `TEXT NULL` | The proposer's free-form reasoning |
| `adjudicator_model` | `TEXT NULL` | |
| `adjudicator_agrees` | `BOOLEAN NULL` | |
| `adjudicator_rationale` | `TEXT NULL` | |
| `detector_version` | `TEXT NOT NULL` | |
| `proposed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `decided_at` | `TIMESTAMPTZ NULL` | |
| `decided_by` | `TEXT NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- `lesson_relations_not_self`: `from_lesson_id <> to_lesson_id`
- `lesson_relations_adjudicator_not_proposer`: `adjudicator_model IS NULL OR proposer_model IS NULL OR adjudicator_model <> proposer_model` — a model never adjudicates its own proposal
- `uq_lesson_relations_from_to_kind`: UNIQUE `(from_lesson_id, to_lesson_id, kind_id)`

**Indexes:**
- `idx_lesson_relations_from_lesson_id`: `from_lesson_id` (BTree)
- `idx_lesson_relations_to_lesson_id`: `to_lesson_id` (BTree)
- `kind_id`/`method_id`/`status_id` are deliberately unindexed: static lookup tables

**Triggers:** `lesson_relation_file_queue` (`20260920000009_lesson_file_sync.sql`) re-queues the
`from` lesson's portable file in `memory.lesson_file_changes` on every insert, update or delete;
`trg_lesson_relations_change_generation` / `_upd` (`20261102260000_change_generations.sql`) bump the
`lessons` row of `memory.change_generations`.

---

### `memory.lesson_relation_kinds`

Lookup for `memory.lesson_relations.kind_id`. Added in `20260729020000_memory_lesson_relations.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `supersedes`(1) `refines`(2) `recurs`(3) `contradicts`(4).

---

### `memory.lesson_relation_methods`

Lookup for `memory.lesson_relations.method_id`: how the relation was established. Rows 1–4 seeded in
`20260729020000_memory_lesson_relations.sql`; `write_time` and `rewritten` added by
`20261014190000_lesson_shape_findings.sql` for relations written with the lesson itself
(`store/persistence/lessons/write_time.rs`, `shape_apply.rs`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `declared`(1) `artifact_reference`(2) `adjudicated`(3) `operator`(4) `write_time`(5)
`rewritten`(6).

---

### `memory.lesson_relation_statuses`

Lookup for `memory.lesson_relations.status_id`. Added in `20260729020000_memory_lesson_relations.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `proposed`(1) `confirmed`(2) `rejected`(3) `contested`(4).

---

### `memory.consolidation_runs`

Added in `20260729010000_memory_consolidation_audit.sql`. One row per consolidation pass of the lesson
worker: `session` (`lesson_worker/session_consolidation.rs`), `nightly` (`lesson_worker/nightly.rs`) or
`gc` (`lesson_worker/gc.rs`). Runs exist only to group and time; all meaning lives on
`memory.consolidation_actions`, and there are deliberately no count columns here (totals are computed
on read). The row is inserted as `running` at pass start (`start_consolidation_run`,
`store/persistence/consolidation_audit.rs`) and updated at pass end (`finish_consolidation_run`), so a
crashed pass shows as a stale running row; the already-ran-today gate is `status IN (completed, partial)`
so a failed run does not block a retry. Read through `store/persistence/consolidation_reads/` for
`GET /api/memory/consolidation/runs` and `GET /api/memory/consolidation/runs/{id}`, by the Session Flow
projection (`session_flow/mike_memory.rs`) and the trace export (`api/session_flow/export_trace.rs`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `run_kind_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_run_kinds(id)` |
| `trigger_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_triggers(id)` |
| `status_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_run_statuses(id)` |
| `started_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `finished_at` | `TIMESTAMPTZ NULL` | NULL exactly while `status_id = running`; set once the pass completes, partially completes, or fails |
| `error` | `TEXT NULL` | |

**Constraints:**
- `consolidation_runs_finished_consistent`: `(status_id = 1) = (finished_at IS NULL)`

**Indexes:**
- `idx_consolidation_runs_started_at`: `started_at DESC` (BTree)
- `run_kind_id`/`trigger_id`/`status_id` are deliberately unindexed: static lookup tables

**Triggers:** `trg_consolidation_runs_row_changed` (insert/delete) and
`trg_consolidation_runs_row_changed_update` (update, only when a value changed) call
`mike.notify_row_changed_consolidation_runs()`, which `pg_notify`s `proxy.row_changed` with
`{table: 'consolidation_runs', id, op}` and no `session_id` (the table is global).

---

### `memory.consolidation_run_kinds`

Lookup for `memory.consolidation_runs.run_kind_id`. Added in `20260729010000_memory_consolidation_audit.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `session`(1) `nightly`(2) `gc`(3).

---

### `memory.consolidation_triggers`

Lookup for `memory.consolidation_runs.trigger_id`: what started the pass. Added in
`20260729010000_memory_consolidation_audit.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `schedule`(1) `startup_catchup`(2) `manual`(3).

---

### `memory.consolidation_run_statuses`

Lookup for `memory.consolidation_runs.status_id`. Added in `20260729010000_memory_consolidation_audit.sql`.
`running` is a durable in-flight marker, not just a terminal label (see `memory.consolidation_runs`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `running`(1) `completed`(2) `partial`(3) `failed`(4).

---

### `memory.consolidation_actions`

Added in `20260729010000_memory_consolidation_audit.sql`. One row per lesson per consolidation decision:
the decision record of what a pass evaluated, how it decided and against what. Identity and text columns
are denormalised on purpose, because GC hard-deletes the lesson row: the `_uuid`/`_text`/`files_touched`
copies are what survive the delete, and `lesson_text` carries the full text on every row, never sliced.
Written by `record_consolidation_actions` (`store/persistence/consolidation_audit/record_actions.sql`) from
`store/persistence/lessons/consolidation.rs` and `retention.rs`; `project_merge.rs` re-points
`project_id` when projects merge. Read through `store/persistence/consolidation_reads/` for
`GET /api/memory/decisions` (the run-agnostic newest-first decision feed), `GET /api/lessons/{id}/history`,
`GET /api/sessions/{id}/memory-actions` and the run detail route, and by the Session Flow projection
(`session_flow/mike_memory.rs`, `session_flow/cursor.rs`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `run_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_runs(id)` ON DELETE CASCADE |
| `lesson_id` | `BIGINT NULL` | FK → `memory.lessons(id)` ON DELETE SET NULL |
| `lesson_uuid` | `UUID NOT NULL` | Survives the lesson's hard delete |
| `project_id` | `BIGINT NULL` | FK → `projects.projects(id)` ON DELETE SET NULL |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL |
| `session_uuid` | `UUID NULL` | Denormalised session UUID; what the row-change notification carries |
| `source_turn_id` | `BIGINT NULL` | FK → `memory.operator_turns(id)` ON DELETE SET NULL |
| `files_touched` | `TEXT[] NULL` | |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL |
| `lesson_text` | `TEXT NOT NULL` | The full lesson text being judged, on every action row |
| `action_kind_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_action_kinds(id)` |
| `outcome_id` | `BIGINT NOT NULL` | FK → `memory.consolidation_outcomes(id)` |
| `error` | `TEXT NULL` | The specific skip/failure cause (empty embedding, embed error, neighbour search error, …) |
| `before_stage_id` | `BIGINT NULL` | FK → `memory.lesson_stages(id)` |
| `after_stage_id` | `BIGINT NULL` | FK → `memory.lesson_stages(id)` |
| `decided_by_id` | `BIGINT NULL` | FK → `memory.consolidation_decided_by(id)` |
| `distance` | `REAL NULL` | Measured cosine distance at decision time |
| `threshold` | `REAL NULL` | The threshold in effect at decision time, so a later config change cannot rewrite history |
| `embedding_model` | `TEXT NULL` | |
| `counterpart_lesson_id` | `BIGINT NULL` | FK → `memory.lessons(id)` ON DELETE SET NULL. The neighbour this lesson was judged against |
| `counterpart_uuid` | `UUID NULL` | |
| `counterpart_text` | `TEXT NULL` | The neighbour's text, so both sides of the decision are visible |
| `runners_up` | `JSONB NOT NULL DEFAULT '[]'` | Ranks 2–3 of the compared neighbour set: `[{lesson_id, distance}, …]` |
| `rank_detail` | `JSONB NULL` | Trim-only evidence: the `consolidation_count`/`created_at` rank and the excess `n` that made the row a target |
| `injection_count` | `INTEGER NULL` | How many times the lesson had been injected as of this decision; the only place that evidence survives a GC delete |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- `consolidation_actions_outcome_error_consistent`: `(outcome_id = 1) = (error IS NULL)`

**Indexes:**
- `idx_consolidation_actions_run_id`: `run_id` (BTree)
- `idx_consolidation_actions_lesson_id`: `lesson_id` (BTree)
- `idx_consolidation_actions_session_id`: `session_id` (BTree)
- `idx_consolidation_actions_source_turn_id`: `source_turn_id` (BTree)
- `idx_consolidation_actions_project_created`: `(project_id, created_at DESC)` (BTree)
- `idx_consolidation_actions_counterpart_lesson_id`: `counterpart_lesson_id` (BTree)
- `idx_consolidation_actions_agent_id`: `agent_id` (BTree)
- `idx_consolidation_actions_files_touched`: `files_touched` (GIN)
- `idx_consolidation_actions_created_at`: `created_at DESC` (BTree) — the run-agnostic decision feed
- `action_kind_id`/`outcome_id`/`before_stage_id`/`after_stage_id`/`decided_by_id` are deliberately unindexed: static lookup tables

**Triggers:** `trg_consolidation_actions_row_changed` (insert/delete) and
`trg_consolidation_actions_row_changed_update` (update, only when a value changed) call
`mike.notify_row_changed_consolidation_actions()`, which `pg_notify`s `proxy.row_changed` with
`{table: 'consolidation_actions', id, session_id: session_uuid, op}`.

---

### `memory.consolidation_action_kinds`

Lookup for `memory.consolidation_actions.action_kind_id`. Added in
`20260729010000_memory_consolidation_audit.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `promoted_to_daily`(1) `promoted_to_long_term`(2) `archived_duplicate`(3)
`archived_trimmed`(4) `archived_expired`(5) `deleted_expired`(6) `evaluated`(7).

---

### `memory.consolidation_outcomes`

Lookup for `memory.consolidation_actions.outcome_id`. Added in
`20260729010000_memory_consolidation_audit.sql`. `applied` is the only outcome with a NULL `error`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `applied`(1) `failed`(2) `skipped`(3).

---

### `memory.consolidation_decided_by`

Lookup for `memory.consolidation_actions.decided_by_id`: the rule that decided the action. Added in
`20260729010000_memory_consolidation_audit.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `embedding_distance`(1) `rank_order`(2) `expiry`(3).

---

### `memory.lesson_injections`

Added in `20260729010000_memory_consolidation_audit.sql`. Live attribution: which request got which lesson
injected. CASCADE (not SET NULL) on `lesson_id`/`request_id` because this table tracks rows that still
exist; it is not an audit trail meant to survive its subjects' deletion
(`memory.consolidation_actions.injection_count` carries that evidence past a GC delete). The proxy's lesson
stage writes a row per injected lesson (`record_lesson_injections`,
`store/persistence/lesson_injections/record.rs`, `origin = 'proxy'`); when the mike mod delivered the
lessons as hook context instead, the proxy finds `[lesson:<id>]`-tagged text already in the client's
request and writes one row per session and lesson on the first request that carried it
(`record_mod_lesson_injections`, `mod_origin.rs`, `origin = 'mod'`); `backfill.rs` is the historical
lesson→request backfill. Read by `GET /api/memory/lesson-outcomes` (`outcomes.rs`),
`GET /api/sessions/{id}/lessons` (`session.rs`), the arm comparison (`arm_comparison.rs`), the
improvement exposures queries (`improvement/lesson_exposures*.sql`), `consolidation_reads/lesson_injections.sql`,
`session_delivery/evidence.rs` and the Session Flow projection (`session_flow/mike_requests.rs`, `cursor.rs`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `lesson_id` | `BIGINT NOT NULL` | FK → `memory.lessons(id)` ON DELETE CASCADE |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL |
| `injected_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `lane_id` | `BIGINT NOT NULL` | FK → `memory.lesson_injection_lanes(id)` |
| `persona` | `TEXT NULL` | |
| `origin` | `TEXT NOT NULL DEFAULT 'proxy'` | Added by `20261014220000_lesson_injection_origin.sql`. `proxy`: the proxy's lesson stage added the text to the outgoing request. `mod`: the mike mod added it as hook context and the client sent it |

**Constraints:**
- `lesson_injections_origin_check`: `origin IN ('proxy', 'mod')`

**Indexes:**
- `idx_lesson_injections_lesson_id`: `lesson_id` (BTree)
- `idx_lesson_injections_request_id`: `request_id` (BTree)
- `idx_lesson_injections_session_injected`: `(session_id, injected_at DESC)` (BTree)
- `idx_lesson_injections_lesson_request`: UNIQUE `(lesson_id, request_id)` (`20260729020000_memory_lesson_relations.sql`) — the `ON CONFLICT` arbiter for the backfill and the mod recorder
- `lane_id` is deliberately unindexed: two-row static lookup table

**Triggers:** `trg_lesson_injections_row_changed` (insert/delete) and
`trg_lesson_injections_row_changed_update` (update, only when a value changed) call
`mike.notify_row_changed_lesson_injections()`, which `pg_notify`s `proxy.row_changed` with
`{table: 'lesson_injections', id, session_id, op}`; `session_id` is resolved from `session_id` when set,
else through `request_id`.

---

### `memory.lesson_injection_lanes`

Lookup for `memory.lesson_injections.lane_id`: which request lane the lesson was injected into. Added in
`20260729010000_memory_consolidation_audit.sql`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `primary`(1) `subagent`(2).

---

### `memory.lesson_delivery_arms`

Added in `20261014180000_mod_lesson_delivery.sql`, with the `mod_lesson_delivery` feature toggle (global,
default off). While the toggle is on, the mike mod adds lessons as hook context at prompt submit and the
proxy stops injecting lessons into the sessions the mod serves; a stable share of sessions stays on proxy
injection as the control group. One row per session that asked, recording which arm it is in and what the
mod delivered to it. Written by `record_lesson_delivery`
(`store/persistence/lesson_injections/delivery_arm.rs`) from `POST /api/lessons/for-prompt`; read by
`compare_lesson_delivery_arms` (`arm_comparison.rs`) for `GET /api/lessons/delivery-comparison`, the
weekly audit's bounded measurement of the two arms.

| Column | Type | Purpose |
|---|---|---|
| `session_uuid` | `UUID PRIMARY KEY` | |
| `arm` | `TEXT NOT NULL` | CHECK `IN ('mod', 'control')` |
| `decided_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_delivery_at` | `TIMESTAMPTZ NULL` | Set only by a delivery that carried at least one lesson |
| `deliveries` | `INTEGER NOT NULL DEFAULT 0` | Count of deliveries that carried at least one lesson |
| `lessons_delivered` | `INTEGER NOT NULL DEFAULT 0` | Running total of lessons delivered |
| `bytes_delivered` | `BIGINT NOT NULL DEFAULT 0` | Running total of bytes delivered |

**Indexes:**
- `idx_lesson_delivery_arms_decided_at`: `decided_at DESC` (BTree)

---

### `memory.change_generations`

Added in `20261102260000_change_generations.sql`. A change counter per derived dataset, bumped by
row-level triggers on the tables the dataset reads. Workers that cache a dataset in memory read one row by
primary key to learn whether to reload, instead of hashing the source tables on every tick. The UPDATE
triggers fire only when a watched value really changed, so no-op writes (re-projection, upserts that change
nothing, `last_seen` bumps) leave the counters alone. `memory.bump_change_generation()` is the trigger
function; its arguments name the rows to bump. Read by `store/persistence/lessons/injectable.rs` (the
`lessons` row, together with a count of lessons past `expires_at`, since a lesson can leave the injectable
set by time without a write), `lessons/vectors.rs` (`lessons`) and `project_knowledge/session_scope.rs`
(`session_projects`).

| Column | Type | Purpose |
|---|---|---|
| `name` | `TEXT PRIMARY KEY` | The derived dataset |
| `generation` | `BIGINT NOT NULL DEFAULT 0` | Incremented on every relevant change |
| `changed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

Seeded rows: `lessons`, `session_projects`; `thread_cards` and `thread_card_sources` are added by
`20261109120000_thread_cards.sql`. `thread_card_sources` is read every 10 s by the memory-keeper card
worker (`store/persistence/work_threads/cards/scan.rs`); `thread_cards` tells each machine's card
snapshot to reload.

**Triggers that bump it:**
- `lessons`: `trg_lessons_change_generation`/`_upd` on `memory.lessons`, `trg_lesson_relations_change_generation`/`_upd` on `memory.lesson_relations`, `trg_lesson_shape_findings_change_generation`/`_upd` on `memory.lesson_shape_findings`
- `lessons` and `session_projects`: `trg_projects_path_change_generation`/`_upd` on `projects.projects` (insert, delete, change of `project_path`)
- `session_projects`: `trg_sessions_project_change_generation`/`_del`/`_upd` on `sessions.sessions` (rows with a `location_uuid` or `project_resolution = 'operator'`; updates of `project_id`, `location_uuid`, `project_resolution`)
- `thread_cards`: `trg_thread_cards_change_generation`/`_upd` on `work.thread_cards` (insert, delete, and an update that changes `card`, `text`, `content_hash` or `refs`; advancing `source_max_at` or `built_at` alone does not bump it)
- `thread_card_sources`: `trg_threads_card_sources`/`_upd` on `work.threads` (any change), `trg_thread_state_card_sources`/`_upd` on `work.thread_state` (the state fields, `files` and `updated_at`; not the embedding columns), `trg_thread_log_card_sources`/`_upd` on `work.thread_log` (any change), `trg_thread_sessions_card_sources`/`_upd` on `work.thread_sessions` (insert, delete, a change of `contribution`; not `last_seen_at`)

The same migration adds `idx_lessons_expires_at` on `memory.lessons (expires_at) WHERE expires_at IS NOT NULL`
for the expiry count.

---

### `memory.keeper_supplies`

Added in `20261109121500_keeper_supplies.sql`. What memory-keeper supplied to each session: one row per
(session, thread, card text), recorded after the block was returned so the route's latency does not include
it. A repeat of the same card for the same session is not recorded again: the writer inserts with
`WHERE NOT EXISTS` over `(session_uuid, thread_id, content_hash)`, served by the `(session_uuid, thread_id)`
index; there is no UNIQUE or index on `content_hash`. The source of the supply rate and of resolution
accuracy. No writer exists yet; the supply path that records into it is the next memory-keeper step.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | |
| `session_uuid` | `UUID NOT NULL` | Soft reference to `sessions.sessions.session_uuid` |
| `thread_id` | `BIGINT NULL` | FK → `work.threads(id)` ON DELETE CASCADE; NULL for a control-arm session or a prompt that resolved to no thread |
| `content_hash` | `TEXT NULL` | `work.thread_cards.content_hash` of the card supplied; not indexed |
| `carrier` | `TEXT NOT NULL` | CHECK `tether`, `mod`, `proxy` |
| `arm` | `TEXT NOT NULL` | CHECK `keeper`, `control` |
| `resolution` | `TEXT NOT NULL` | CHECK `one`, `several`, `none` |
| `signals` | `JSONB NOT NULL DEFAULT '{}'` | The resolver signals behind the resolution |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_keeper_supplies_session_thread`: `(session_uuid, thread_id)` (BTree)

---

















### `memory.session_embeddings`

Added in migration `20260901000001_session_embeddings.sql`. One vector per session with embedded turns.
Until the session judge writes a narrative overview, the vector is the L2-normalised mean of the session's
`memory.operator_turn_chunks` embeddings (`source = 'centroid'`); once an overview exists, the vector is the
overview's own embedding (`source = 'overview'`). Both live in the same 768-dim space of the configured `embedding.model`.
Keyed on `session_id uuid`, referencing `sessions.sessions(session_uuid)` — not the bigint surrogate `id`
every other `memory.*` table FKs against — because every caller (the embed worker sweep, the two
embedding-graph REST routes, the session judge) already holds the session's UUID.

Staleness rule (enforced in Rust by `PersistHandle::list_sessions_needing_embedding`, not by a DB
constraint): a session is **missing** when it has at least one embedded turn chunk and no row here, and
**stale** when its row's `source_updated_at` is older than `GREATEST` of the session's
`memory.session_chunk_marks.chunks_embedded_at` and `sessions.sessions.overview_generated_at`.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `UUID PRIMARY KEY` | FK → `sessions.sessions(session_uuid)` ON DELETE CASCADE |
| `embedding` | `vector(768) NOT NULL` | 768-dim embedding (the configured `embedding.model`) |
| `embedding_model` | `TEXT NOT NULL` | Model identifier |
| `source` | `TEXT NOT NULL` | `CHECK (source IN ('overview', 'centroid'))` |
| `chunk_count` | `INTEGER NOT NULL DEFAULT 0` | Turn-chunk count the centroid was computed over (0 for an overview row) |
| `source_updated_at` | `TIMESTAMPTZ NOT NULL` | Timestamp of the source content this vector reflects — drives the staleness rule |
| `embedded_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When this row was last written |

**Indexes:**
- `idx_session_embeddings_hnsw`: `embedding vector_cosine_ops` HNSW m=16 ef=64 — cosine ANN recall

### `memory.session_chunk_marks`

Added in migration `20261102370000_session_chunk_marks.sql`. One row per session with embedded turn
chunks: the newest chunk `embedded_at`, so the session embedding sweep reads one narrow row per session
instead of aggregating every chunk on each pass. The statement-level trigger
`trg_operator_turn_chunks_mark_session` on `memory.operator_turn_chunks` inserts or advances it for every
chunk writer; it never moves backwards. `session_chunk_centroid` reports this value as the newest chunk
time, so the sweep records exactly what the staleness rule compares against.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `BIGINT PRIMARY KEY` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `chunks_embedded_at` | `TIMESTAMPTZ NOT NULL` | Newest `embedded_at` among the session's turn chunks |

---

### `memory.project_embeddings`

Added in migration `20260901000001_session_embeddings.sql`. One row per project: the L2-normalised mean of
`memory.session_embeddings` across the project's sessions, with `session_count`. Recomputed by
`PersistHandle::refresh_project_centroids` (one set-based `INSERT ... SELECT ... ON CONFLICT DO UPDATE`) for any
project whose newest session vector is newer than this row, or that has none yet.

| Column | Type | Purpose |
|---|---|---|
| `project_id` | `BIGINT PRIMARY KEY` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `embedding` | `vector(768) NOT NULL` | 768-dim centroid embedding |
| `embedding_model` | `TEXT NOT NULL` | Model identifier |
| `session_count` | `INTEGER NOT NULL` | Sessions the centroid was computed over |
| `embedded_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When this row was last written |

**Indexes:**
- `idx_project_embeddings_hnsw`: `embedding vector_cosine_ops` HNSW m=16 ef=64 — cosine ANN recall

---

### `memory.session_knowledge`

Added in migration `20260901000001_session_embeddings.sql`. One row per judged session: the session judge's
extracted topics and durable learnings, replaced (not appended) on every judge run — this is where session
learnings live for display. **Never copied into `memory.lessons`**: `list_injectable_lessons`
(`lesson_relations.rs`) injects every non-archived lesson stage into future prompts on the project, and this
content is machine-derived, not operator/agent-authored. `topics`, `learnings` and `related_session_ids` are
denormalised arrays (judge-authored display strings and soft session references, never individually
filtered/sorted/joined on) — the same rationale as `memory.operator_turns.tools_used` etc.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `UUID PRIMARY KEY` | FK → `sessions.sessions(session_uuid)` ON DELETE CASCADE |
| `topics` | `TEXT[] NOT NULL DEFAULT '{}'` | Three to five short labels |
| `learnings` | `TEXT[] NOT NULL DEFAULT '{}'` | Zero to five durable claims the session established, learned or corrected |
| `related_session_ids` | `UUID[] NOT NULL DEFAULT '{}'` | The neighbour sessions the judge was shown while writing this row |
| `verdict_id` | `BIGINT NULL` | FK → `session_judges.verdicts(id)` ON DELETE SET NULL |
| `generated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_session_knowledge_verdict_id`: `verdict_id` (BTree)

---

### `memory.knowledge_documents`

Added in `20260920000008_project_knowledge.sql`. One row per file path the collector has reported under a
`memory.knowledge_sources` root; the row is the document's identity across revisions. A complete scan
(`store/persistence/project_knowledge/documents.rs`, `POST /api/knowledge/sources/{id}/scan`) inserts or
un-deletes a row per reported path, appends a `memory.knowledge_revisions` row when the content, title,
kind, status or metadata changed (`ingest.sql`, ignoring `source_git_commit` and `source_worktree_dirty`)
and points `current_revision` at it, and soft-deletes rows whose path the scan did not report. Read by
`read.sql`, `search.sql` and `guidance.sql` (`GET /api/knowledge/read` / MCP `read_project_knowledge`,
`GET /api/knowledge/search` / MCP `search_project_knowledge`), the `memory.knowledge_embedding_sources_v`
view and `machine_merge.rs`.

| Column | Type | Purpose |
|---|---|---|
| `document_uuid` | `UUID PRIMARY KEY DEFAULT gen_random_uuid()` | |
| `source_uuid` | `UUID NOT NULL` | FK → `memory.knowledge_sources(source_uuid)` (NO ACTION) |
| `relative_path` | `TEXT NOT NULL` | Path under the source root |
| `current_revision` | `BIGINT NULL` | The newest revision; see the deferred FK below |
| `deleted_at` | `TIMESTAMPTZ NULL` | Set when a complete scan no longer reports the path; cleared when it reappears |

**Constraints:**
- UNIQUE `(source_uuid, relative_path)`
- `knowledge_current_revision`: FK `(document_uuid, current_revision)` → `memory.knowledge_revisions(document_uuid, id)` DEFERRABLE INITIALLY DEFERRED, so a revision can be inserted and pointed at in one transaction

**Indexes:**
- `knowledge_documents_source`: `(source_uuid, deleted_at)` (BTree)

---

### `memory.knowledge_revisions`

Added in `20260920000008_project_knowledge.sql`. Immutable full source text and metadata of one observed
version of a `memory.knowledge_documents` row; search results refer to these revisions, and generated OKF
bundles are not original sources. Appended by `ingest.sql` (see `memory.knowledge_documents`). The `artifact`
rows of `memory.knowledge_embedding_sources_v` are keyed by revision id, so a new revision is a new
embedding source. Read by `read.sql`, `search.sql`, `guidance.sql`, `pending_embeddings.sql` and
`machine_merge.rs`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `document_uuid` | `UUID NOT NULL` | FK → `memory.knowledge_documents(document_uuid)` (NO ACTION) |
| `title` | `TEXT NOT NULL` | |
| `body` | `TEXT NOT NULL` | Full source text, stored whole |
| `content_hash` | `TEXT NOT NULL` | The collector's hash of the content; a changed hash is a new revision |
| `kind` | `TEXT NOT NULL` | |
| `status` | `TEXT NOT NULL DEFAULT 'unknown'` | |
| `metadata` | `JSONB NOT NULL DEFAULT '{}'` | |
| `observed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `search_tsv` | `TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', title \|\| ' ' \|\| body)) STORED` | Kept for `ts_rank_cd` in `search.sql` |

**Constraints:**
- UNIQUE `(document_uuid, id)` — the target of the deferred `knowledge_current_revision` FK

**Indexes:** none. `knowledge_revision_search` (GIN over `search_tsv`, 338 MB, unusable behind the OR in
`search.sql`) was dropped by `20260930010100_drop_knowledge_revision_search.sql`.

---

### `memory.knowledge_embedding_state`

Created in `20260920000010_knowledge_embeddings.sql` keyed by a text `reference` and a content hash;
reshaped by `20261102210000_knowledge_embedding_typed_sources.sql` to be keyed by `(source_kind, source_id)`
and to detect change from a version the source keeps instead of hashing every source on every read:
`artifact` by the revision id itself, `note` by `workspace.session_notes.updated_at`, `thread` by the
thread's newest `mailbox.messages` id, `lesson` by `memory.lessons.text_md5`. One row per source the
project-knowledge embedding sweep (`crates/mike/src/embed_worker/knowledge.rs`, lease role
`knowledge_embedding`) has attempted. `pending_embeddings.sql` picks sources from
`memory.knowledge_embedding_sources_v` that have no row, a different `model_key`, a drifted version, or a
due retry (`next_attempt_at <= now()` with `attempts < 10` or a `[transient]` error);
`save_knowledge_embeddings` (`store/persistence/project_knowledge/embeddings.rs`) writes the row and its
`memory.knowledge_embedding_chunks`. The knowledge health reads (`embedding_health_all.sql`,
`embedding_health_project.sql`) count ready and failed rows per model, capped at `KNOWLEDGE_COUNT_CAP`;
`search.sql` and `guidance.sql` join it for semantic search.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key (since `20261102210000`) |
| `source_kind` | `TEXT NOT NULL` | CHECK `IN ('artifact', 'lesson', 'note', 'thread', 'thread_entry', 'question_answer', 'unmet_ask', 'correction', 'learning', 'mail')` (session memory kinds since `20261109160000`) |
| `source_id` | `BIGINT NOT NULL` | By kind: `memory.knowledge_revisions.id`, `memory.lessons.id`, `workspace.session_notes.id`, `mailbox.threads.id`, `work.thread_log.id`, `workspace.questions.id`, `memory.turn_verdicts.id` (unmet_ask, correction), `sessions.sessions.id` (learning) or `mailbox.messages.id`; no FK |
| `model_key` | `TEXT NOT NULL` | The embedder the vectors came from; a different key re-embeds the source |
| `completed_at` | `TIMESTAMPTZ NULL` | NULL until the source has vectors |
| `attempts` | `INTEGER NOT NULL DEFAULT 0` | |
| `next_attempt_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_error` | `TEXT NULL` | `[transient]`-prefixed errors retry past the attempt cap |
| `source_updated_at` | `TIMESTAMPTZ NULL` | The note's `updated_at` the vectors match; `note` only |
| `source_seq` | `BIGINT NULL` | The thread's newest message id the vectors match; `thread` only |
| `source_hash` | `TEXT NULL` | The lesson's `text_md5` the vectors match; `lesson` only |

**Constraints:**
- `knowledge_embedding_state_source_kind_check`: the ten kinds above
- `knowledge_embedding_state_version_shape_check`: `(source_kind IN ('note', 'question_answer', 'learning') OR source_updated_at IS NULL) AND (source_kind = 'thread' OR source_seq IS NULL) AND (source_kind = 'lesson' OR source_hash IS NULL)`. Thread entries, verdicts and mail are immutable, so their id is their version; answered questions use `settled_at`, learnings `generated_at`.

**Indexes:**
- `knowledge_embedding_state_source_key`: UNIQUE `(source_kind, source_id)` (BTree)
- `idx_knowledge_embedding_state_failed`: `model_key WHERE last_error IS NOT NULL` (BTree partial, `20261102410001`) — the capped failure count
- `idx_knowledge_embedding_state_ready`: `model_key WHERE completed_at IS NOT NULL` (BTree partial, `20261102410002`) — the capped ready count after a model change

---

### `memory.knowledge_embedding_chunks`

Added in `20260920000010_knowledge_embeddings.sql`; re-keyed from `(reference, ordinal)` to
`(state_id, ordinal)` by `20261102210000_knowledge_embedding_typed_sources.sql`. The chunked text and vector
of one embedded source, written with its `memory.knowledge_embedding_state` row by
`save_knowledge_embeddings` and replaced whole when the source is re-embedded. Read by `search.sql` and
`guidance.sql` for semantic project-knowledge search.

| Column | Type | Purpose |
|---|---|---|
| `state_id` | `BIGINT NOT NULL` | FK → `memory.knowledge_embedding_state(id)` ON DELETE CASCADE |
| `ordinal` | `INTEGER NOT NULL` | Chunk position within the source |
| `body` | `TEXT NOT NULL` | The chunk's text |
| `embedding` | `public.vector(768) NOT NULL` | |

**Primary key:** `(state_id, ordinal)`.

**Indexes:**
- `idx_knowledge_embedding_chunks_embedding`: `embedding vector_cosine_ops` HNSW m=16 ef=64 (`20261109100001`) — nearest-first chunk search in `search.sql`, which reads the nearest 200 chunks of the project with pgvector's iterative scan and keeps only references within 0.08 of the best score and at or above 0.6

---





### `memory.search_events`

Added in `20261109140000_search_events.sql`. One row per `search_project_knowledge` or
`recall_similar_turns` call, written after the answer is built by a task spawned on `AppState.tracker`
(`search_events/record.rs`; SQL `store/persistence/search_events/record.sql`). The
`search_events` worker (`search_events/worker.rs`, lease role `search_events_used`) fills the used columns
with `store/persistence/search_events/mark_used.sql`. Deleting a session deletes its events.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE; NULL when the caller sent no session id or the id is unknown |
| `request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request whose response emitted the calling `tools.uses` row, else the session's newest request |
| `tool` | `TEXT NOT NULL` | CHECK `IN ('search_project_knowledge', 'recall_similar_turns')` |
| `query` | `TEXT NOT NULL` | The query as sent, `''` when browsing |
| `kind_filter` | `TEXT NULL` | The `kind` argument of a knowledge search |
| `returned` | `JSONB NOT NULL DEFAULT '[]'` | Array, in the order the caller saw it, of `{reference, score, rerank_score?}`; `score` is the plain fused rank (knowledge) or rank (turns) |
| `rerank_state` | `TEXT NOT NULL` | CHECK `IN ('reranked', 'loading', 'off', 'unavailable')`, as in the answer's `rerank.state` |
| `used_references` | `TEXT[] NOT NULL DEFAULT '{}'` | Returned references the same session read with `read_project_knowledge` or `get_operator_turn`, or named in a turn's final reply, from the request after the search through the fifth following operator turn |
| `used_checked_at` | `TIMESTAMPTZ NULL` | When the worker last evaluated the row; open rows are taken oldest-checked first |
| `used_final_at` | `TIMESTAMPTZ NULL` | Set when the five following turns have closed or the row is a day old; the used columns no longer change |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_search_events_session`: `session_id WHERE session_id IS NOT NULL` (BTree partial) — the cascade from `sessions.sessions`
- `idx_search_events_request`: `request_id WHERE request_id IS NOT NULL` (BTree partial) — the `SET NULL` from `requests.requests`
- `idx_search_events_open`: `(used_checked_at NULLS FIRST, created_at) WHERE used_final_at IS NULL AND session_id IS NOT NULL AND request_id IS NOT NULL` (BTree partial) — the worker's batch

---

### `memory.knowledge_sources` (collector health columns)

The table itself was added in `20260920000008_project_knowledge.sql`: one row per explicitly registered
collector root at a `projects.locations` checkout, unique on `(location_uuid, name)`, with `generation` and
`last_scan_at` advanced only by a successful complete scan. Migration `20260922140000_knowledge_source_health.sql`
adds what the collector reports about itself. Every new column is nullable, because "the collector has not
reported yet" must read as unknown, never as healthy or failed, and the server never derives a cause from an
absent success.

| Column | Type | Purpose |
|---|---|---|
| `last_heartbeat_at` | `TIMESTAMPTZ NULL` | When the collector last reported for this source (a heartbeat, or a failure report). A report time, never an online/offline verdict |
| `last_failure_at` | `TIMESTAMPTZ NULL` | When the collector last reported an aborted scan |
| `last_failure` | `TEXT NULL` | The collector's full error text for that failure, stored whole |
| `last_failure_generation` | `BIGINT NULL` | The source's `generation` when the failure was recorded, read from the row by the server. Below `generation` means a later scan succeeded |

**Constraint:** `knowledge_sources_failure_report` — the three failure columns are all null or all set, the
text is non-blank and the generation is `>= 0`. A successful scan leaves the last failure in place as history.

Read by `PersistHandle::knowledge_project_health`, which returns one entry per project location with its
sources and a `lessons` object (`pending`: changes in `memory.lesson_file_changes` with no receipt at that
location for the current version, so it includes conflicted ones; `conflicted`: rows in
`memory.lesson_file_conflicts`).

---

### `memory.lesson_file_changes`

Added in `20260920000009_lesson_file_sync.sql`. Transactional coalescing outbox of portable lesson
revisions, including deletion tombstones: one row per lesson, re-projected from `memory.portable_lesson_v`
by trigger `lesson_file_queue` on `memory.lessons` and `lesson_relation_file_queue` on
`memory.lesson_relations` (`memory.queue_lesson_file()`, `memory.queue_lesson_relation_file()`).
`version` increments only when the projected payload or project actually changed; a deleted lesson leaves
a payload of `{lesson_uuid, deleted: true}`. The migration seeds a row for every lesson that existed.
Filesystem delivery is acknowledged per location and version in `memory.lesson_file_receipts`. Read by
`lesson_files` (`store/persistence/project_knowledge/lesson_sync.rs`, `GET /api/knowledge/lessons`), which
hands a location every change with the version it last acknowledged; by the knowledge health read
(`health.sql`, "pending" = changes with no receipt at the current version); `project_merge.rs` re-points
`project_id` when projects merge.

| Column | Type | Purpose |
|---|---|---|
| `lesson_uuid` | `UUID PRIMARY KEY` | `memory.lessons.uuid`; no FK, so the tombstone outlives the lesson |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` (NO ACTION) |
| `version` | `BIGINT NOT NULL DEFAULT 1` | Bumped on each real change of the payload |
| `payload` | `JSONB NOT NULL` | The portable lesson document from `memory.portable_lesson_v`, or the deletion tombstone |
| `changed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `lesson_file_changes_project`: `(project_id, lesson_uuid)` (BTree)

---

### `memory.lesson_file_receipts`

Added in `20260920000009_lesson_file_sync.sql`. Which version of each lesson file a location's collector
has written to disk: one row per `(location, lesson)`, upserted by `acknowledge_lesson_files`
(`store/persistence/project_knowledge/lesson_sync.rs`, `POST /api/knowledge/lessons/ack`), which also
clears the matching `memory.lesson_file_conflicts` row. A change whose receipt is behind
`memory.lesson_file_changes.version` is pending for that location. `machine_merge.rs` moves receipts when
machines merge.

| Column | Type | Purpose |
|---|---|---|
| `location_uuid` | `UUID NOT NULL` | FK → `projects.locations(location_uuid)` (NO ACTION) |
| `lesson_uuid` | `UUID NOT NULL` | FK → `memory.lesson_file_changes(lesson_uuid)` (NO ACTION) |
| `version` | `BIGINT NOT NULL` | The `lesson_file_changes.version` the location wrote |
| `written_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Refreshed on each acknowledgement |

**Primary key:** `(location_uuid, lesson_uuid)`.

---



### `memory.lesson_file_conflicts`

Added in migration `20260922140000_knowledge_source_health.sql`. One row per `(location, lesson)` that a
collector reported as conflicting: the checkout's lesson file was edited locally while the server copy moved
on, so the collector preserved the file and wrote nothing. `memory.lesson_file_changes` and
`memory.lesson_file_receipts` cannot express this, because only the collector sees the local edit.

| Column | Type | Purpose |
|---|---|---|
| `location_uuid` | `UUID NOT NULL` | FK → `projects.locations(location_uuid)` ON DELETE CASCADE; leads the primary key, so it is also the FK index |
| `lesson_uuid` | `UUID NOT NULL` | FK → `memory.lesson_file_changes(lesson_uuid)` ON DELETE CASCADE |
| `server_version` | `BIGINT NOT NULL` | CHECK `>= 1`; `lesson_file_changes.version` when the conflict was reported |
| `detected_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Refreshed when the same conflict is reported again |

**Primary key:** `(location_uuid, lesson_uuid)`. **Indexes:** `lesson_file_conflicts_lesson` on `lesson_uuid`
(the second FK). A row is deleted by `acknowledge_lesson_files` once the location's receipt for that lesson
reaches `server_version`.

---

### `memory.operator_turns_v`

Added in migration `20260610000004_capture_inspection_views.sql`, one of four capture-inspection views.
`memory.operator_turns` with its outcome code resolved, session/project identity joined
in, and the start request's `human_prompt` + `turn_origin` for orientation.

Columns: `id`, `session_id`, `session_uuid`, `project_id`, `project_path`, `start_request_id`,
`end_request_id`, `outcome_evidence_request_id`, `outcome`, `user_prompt`, `prompt_len` (char length of
`user_prompt`), `start_human_prompt`, `start_turn_origin`, `final_assistant_text`, `tool_call_count`,
`tool_error_count`, `weasel_findings_count`, `total_input_tokens`, `total_output_tokens`, `tools_used`,
`agents_dispatched`, `files_touched`, `classified_at`, `created_at`, `closed_at`, `embedded_at`.

Not to be confused with `mike.operator_turn_aggregates_v`/`mike.operator_turn_text_v` (Read-API views,
below) — those recompute live for the single open turn; this view always reads the base table's stored
(possibly NULL-while-open) columns directly.

---

### `memory.live_turn_verdicts_v`

Added in `20261109160000_session_memory_knowledge_kinds.sql`. The newest live (`mode = 'live'`) verdict
of each operator turn, by `(created_at, id)`, with the turn's `project_id`; an anti-join so a project
filter reaches `memory.operator_turns`. Read by `search.sql` (kinds `unmet_ask`, `correction`) and
`memory.knowledge_embedding_sources_v`.

Columns: `id`, `operator_turn_id`, `session_id`, `reaction`, `unmet_ask`, `rationale`, `created_at`,
`project_id`.

The same migration adds `memory.session_learnings_text(text[])`, an immutable `array_to_string(…, E'\n')`
so session learnings can carry an expression index, and these indexes:
- `idx_turn_verdicts_live_text`: GIN over `to_tsvector('english', coalesce(unmet_ask, '') || ' ' || coalesce(rationale, ''))` `WHERE mode = 'live'`
- `idx_turn_verdicts_live_turn_newest`: `(operator_turn_id, created_at DESC, id DESC) WHERE mode = 'live'`, for the newest-verdict anti-join
- `idx_session_knowledge_learnings_text`: GIN over `to_tsvector('english', memory.session_learnings_text(learnings))`
- `idx_mailbox_messages_sent_text`: GIN over `search_tsv` `WHERE from_session_id IS NOT NULL`, for the `mail` kind

---

### `memory.turn_verdicts`

Every judgement of an operator turn, append-only: code inserts and reads, never updates or deletes; rows go only when their turn or session is deleted (FK cascade). `mode` is `live` (the production judge), `shadow` (a candidate on a sample, never projected) or `eval` (an offline eval run item). `projected = true` marks the live verdict whose insert also wrote `operator_turns.outcome_id`, in the same statement. A row has one of two shapes: `reaction` and `delivery` (two-axis rubrics) or `legacy_label` (the r3 rubric). `rubric_version` is the effective version: the rubric id, or `<id>+<sha12>` when the prompt in force differs from the built-in text. `input_text` is the exact user message sent, whole; `generation_id`, `cost_usd`, the token counts and `latency_ms` come from the call itself. `bundle_id` may be NULL only on live rows. `failure_tags` and `severity` are NULL unless the rubric asks for them. Indexes: `(operator_turn_id, mode, bundle_id)`, `(session_id, created_at DESC)`, `(bundle_id, mode, created_at)`; `generation_id` is deliberately not indexed.

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `operator_turn_id` | bigint NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE |
| `session_id` | bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE |
| `mode` | text NOT NULL CHECK (mode IN ('live', 'shadow', 'eval')) |
| `bundle_id` | bigint REFERENCES eval.bundles(id) |
| `rubric_version` | text NOT NULL |
| `input_builder_version` | text NOT NULL |
| `slices` | text[] NOT NULL DEFAULT '{}' |
| `model` | text NOT NULL |
| `source` | text NOT NULL DEFAULT 'llm' CHECK (source IN ('llm', 'deterministic')) |
| `reaction` | text CHECK (reaction IN ('corrected', 'redirected', 'accepted', 'no_response', 'unclear')) |
| `delivery` | text CHECK (delivery IN ('shipped', 'delivered', 'partial', 'none', 'question', 'not_applicable')) |
| `legacy_label` | text CHECK (legacy_label IN ('accepted', 'shipped', 'corrected', 'abandoned', 'unknown')) |
| `confidence` | real NOT NULL CHECK (confidence >= 0 AND confidence <= 1) |
| `rationale` | text NOT NULL |
| `unmet_ask` | text NOT NULL |
| `evidence_quality` | text NOT NULL CHECK (evidence_quality IN ('verified', 'asserted', 'absent')) |
| `projected` | boolean NOT NULL DEFAULT false |
| `next_genuine_request_id` | bigint |
| `as_of` | timestamptz NOT NULL |
| `input_text` | text NOT NULL |
| `generation_id` | text |
| `cost_usd` | double precision |
| `failure_tags` | text[] |
| `severity` | text CONSTRAINT turn_verdicts_severity_check CHECK (severity IN ('none', 'minor', 'major', 'severe')) |
| `prompt_tokens` | integer |
| `completion_tokens` | integer |
| `latency_ms` | integer |
| `created_at` | timestamptz NOT NULL DEFAULT now() |

### `memory.turn_facts`

Added in migration `20260924150000_turn_facts.sql`. One row per closed `memory.operator_turns` row (operator turns
and machine segments), built from captured data by `crates/mike/src/dossier/build` with no LLM call. A row is a
cache of facts as of `built_at`; the `*_at_close` columns are as of the turn's `closed_at` and are written once,
on insert.

| Column | Type | Notes |
|---|---|---|
| `operator_turn_id` | `BIGINT PK` | FK → `memory.operator_turns(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `builder_version` | `TEXT NOT NULL` | `turn-facts-v1`; a bump makes every row stale |
| `built_at` | `TIMESTAMPTZ NOT NULL` | the moment the hindsight columns are as of |
| `max_request_id` | `BIGINT NOT NULL` | highest `requests.id` attached to the turn at build; a later attached request makes the text facts stale |
| `prompt_kind` | `TEXT NOT NULL` | effective kind of the opening message as of `built_at`, in J2 codes: `genuine`, `empty`, `peer-message`, `loop-wakeup`, `channel-wake`, `task-notification`, `background-event`, `tool-loaded`, `hook-feedback`, `compaction`, `analysis-probe`, `session-recap` |
| `prompt_rule` | `TEXT` | `a`, `b`, `ab`, `c` or `envelope`; NULL when no rule fired |
| `prompt_kind_at_close` | `TEXT NOT NULL` | the kind as of `closed_at`, written once |
| `prompt_rule_at_close` | `TEXT` | the rule as of `closed_at`, written once |
| `next_state` | `TEXT NOT NULL` | `resolved`, `pending` or `none_session_settled`; CHECK ties `resolved` to a non-NULL `next_genuine_request_id` |
| `next_genuine_request_id` | `BIGINT` | the operator's next genuine message; no foreign key, so a facts write never fails on a pruned row |
| `next_genuine_turn_id` | `BIGINT` | its turn, NULL when it opened no row |
| `next_genuine_at` | `TIMESTAMPTZ` | its `requests.created_at` |
| `next_state_at_close` | `TEXT NOT NULL` | `next_state` as of `closed_at`, written once |
| `next_genuine_request_id_at_close` | `BIGINT` | the next genuine message as of `closed_at`, written once |
| `skipped_before_next` | `JSONB NOT NULL` | counts by J2 code of the messages between this one and the next genuine one |
| `reply_window_segment_ids` | `BIGINT[] NOT NULL` | machine segments between this row and the next genuine message |
| `reclassified_at` | `TIMESTAMPTZ` | last build at which the kind, the rule or a non-NULL next genuine message changed; `pending` becoming `resolved` is resolution, not reclassification |
| `text_chars` | `INTEGER NOT NULL` | `char_length(final_assistant_text)` at build, in characters |
| `text_cut_at` | `INTEGER NOT NULL` | character offset just after the last text block from a non-sidecar request; `text_cut_at < text_chars` exactly when the turn ends in sidecar text |
| `last_text_origin` | `TEXT` | origin of the last text block's request |
| `text_spans` | `JSONB NOT NULL` | runs of consecutive text blocks from one origin, in string order: `origin`, `start`, `end`, `first_request_id`, `last_request_id`, offsets in characters into `final_assistant_text`; no assistant text is copied |
| `stop_request_id` | `BIGINT` | the last primary text-bearing request of the turn |
| `stop_reason` | `TEXT` | its stop reason (`end_turn`, `max_tokens`, `stop_sequence`, `tool_use`, `pause_turn`, `error`, `refusal`), or `aborted` when it has no response |
| `aborted` | `BOOLEAN NOT NULL` | `stop_reason = 'aborted'` or `close_cause = 'abort'` |
| `interrupted` | `BOOLEAN NOT NULL` | a primary request in the turn carried `[Request interrupted by user` |
| `close_cause` | `TEXT NOT NULL` | `next_prompt`, `idle_reap` or `abort` |
| `checks_run` | `JSONB NOT NULL` | one element per Bash check (test, lint, typecheck, build, perf, migrate, gate) with the full command, exit code and rejected, background and pending flags |
| `checks_count` | `INTEGER NOT NULL` | number of elements of `checks_run` |
| `checks_failed` | `INTEGER NOT NULL` | elements with a positive exit code |
| `ended_on` | `TEXT NOT NULL` | how the non-sidecar text ends: `report`, `question`, `handback`, `promise_or_wait`, `other` or `none`; an input for judges and steerers, never a trigger |
| `ended_on_version` | `TEXT NOT NULL` | `ended-on-v1` |
| `session_class` | `TEXT NOT NULL` | `operator`, `fleet_seat`, `template`, `subagent`, `workflow` or `synthetic`, as of `built_at` |
| `session_class_rule` | `TEXT NOT NULL` | which rule of `session-class-v1` fired |

**Indexes:** the primary key; `idx_turn_facts_session` on `(session_id, operator_turn_id)`;
`idx_turn_facts_pending` on `(session_id)` WHERE `next_state = 'pending'`.

The migration also extends `feature_toggles_key_check` with `dossier_builder` (default off, switched on after deploy; local derivation only)
and inserts its `config.feature_toggles` row with `operator_set = false`.

**Steering facts** (migration `20260925000200_turn_facts_steering_facts.sql`) add these columns, written by
`dossier::steering_facts` as of the turn's `closed_at`:

| Column | Type | Meaning |
|---|---|---|
| `wait_backing` | `TEXT` | the strongest backing for a wait at close: `work_item`, `scheduled_wake`, `monitor`, `peer_reply`, `operator_requested`, `unknown` (a Monitor, ScheduleWakeup or CronCreate launch the registry does not parse yet) or `none`; NULL until computed |
| `wait_backing_refs` | `JSONB NOT NULL DEFAULT '[]'` | every backing found, whole, as evidence |
| `observed_after_change` | `BOOLEAN` | NULL when the turn changed nothing; true when a check, run, request, browser read or render with a result followed the last change |
| `last_change_use_id`, `last_change_at` | `BIGINT`, `TIMESTAMPTZ` | the last mutating `tools.uses` row (no foreign key, so retention can delete tool rows) |
| `last_observation_use_id`, `last_observation_at` | `BIGINT`, `TIMESTAMPTZ` | the last observing call after that change |
| `observation_kind` | `TEXT` | `test`, `build`, `lint`, `gate`, `run`, `http`, `browser`, `render`, `deploy_poll` or `read_back` (the session read back what it had just written) |
| `steering_facts_version` | `SMALLINT NOT NULL DEFAULT 0` | 0 until computed; `mike dossier-backfill --stage steering-facts` recomputes rows below the current version |

Every source is bounded by the as-of moment, and none reads a verdict or a label, so a replay at T sees only what existed at T.
The session class stays in `session_class` / `session_class_rule`; its evidence is returned by the REST view
`GET /api/sessions/{id}/turns/{turn_id}/steering-facts?as_of=`, which shows the computed facts next to the persisted row.

---

### `memory.turn_facts_backstop_attempts`

Added in `20261102230000_turn_facts_backstop_attempts.sql`. The dossier backstop used to re-serve every
turn whose captured sources were not yet consistent on each 60 s tick, forever, and such sessions sorted
first and filled the batch. Each time the backstop serves a stale turn, the candidates query
(`store/persistence/turn_facts/candidates.sql`) now records an attempt here and skips the turn until
`retry_after` (60 s, doubling per attempt, capped at 1 h), and for good once `unbuildable_at` is set, which
happens when the attempt count reaches the query's cap parameter. Writing the turn's facts deletes its
row (`turn_facts/upsert.sql`, `turn_facts/write.rs`).

| Column | Type | Purpose |
|---|---|---|
| `operator_turn_id` | `BIGINT PRIMARY KEY` | FK → `memory.operator_turns(id)` ON DELETE CASCADE |
| `attempt_count` | `INTEGER NOT NULL` | CHECK `> 0` |
| `last_attempt_at` | `TIMESTAMPTZ NOT NULL` | |
| `retry_after` | `TIMESTAMPTZ NOT NULL` | The turn is left alone until this time |
| `unbuildable_at` | `TIMESTAMPTZ NULL` | Set once the attempt cap is reached; the turn is then skipped permanently |

---


### `memory.ask_ledger_entries` (widened by `20260924180000_ask_ledger_dossier_entries.sql`)

The ask ledger gains the request that carried each ask and who set its kind. `kind` also accepts
`redirect`, which only the judge writes.

| Column | Type | Purpose |
|---|---|---|
| `request_id` | `BIGINT NULL` | `requests.requests.id` of the prompt; no FK; unique when present (`uq_ask_ledger_entries_request`) |
| `kind_source` | `TEXT NOT NULL DEFAULT 'keyword'` | `keyword`, `judge` or `operator` |
| `keyword_kind` | `TEXT NULL` | the keyword classifier's kind at insert, never changed; NULL before JC03 |
| `kind_updated_at` | `TIMESTAMPTZ NULL` | when a judge or operator last changed `kind` |

`operator_turn_id` is now filled by the dossier builder, which links each row to the turn its
`request_id` opened. The steerer path appends with `request_id`; the builder path adds a row for an
operator turn the steerer missed. The verifier and the intake author read only rows whose request is
classified `operator` (`fetch_ask_ledger_for_model`); the inspection route returns every row with a
`genuine` flag.

### `memory.acceptance_criteria_maps`

Added in `20260820000002_acceptance_criteria_maps.sql` (GitHub issue temet-ai/mike-private#152). One authoring of a session's
acceptance criteria from the operator's accumulated words only, by a worker that never sees the session's
own restatement of the ask (`crates/mike/src/steerer/intake_criteria/`, called from
`steerer/fleet/ask_ledger.rs` after an ask-ledger entry when `should_reauthor` says the newest entry is
beyond `last_ask_entry_id`). The map is re-authored per operator turn as a new `version` rather than
overwritten, so the change between versions stays visible; the newest version is what a stop is graded
against (`steerer/fleet/verifier_judge.rs`, `store/persistence/verifier_replay.rs`). Written by
`store/persistence/acceptance_criteria.rs`; read by `GET /api/sessions/{id}/acceptance-criteria` and the
ask-ledger recount SQL (`ask_ledger_backfill/`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `version` | `INTEGER NOT NULL` | 1 for the first authoring, incrementing per re-author |
| `ask_entry_count` | `INTEGER NOT NULL` | How many ask-ledger entries the worker was shown |
| `model` | `TEXT NULL` | The model that authored it |
| `output_tokens` | `INTEGER NULL` | Output tokens billed; NULL when the provider reported no usage |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_ask_entry_id` | `BIGINT NULL` | Added by `20261007100000_acceptance_criteria_last_ask_entry.sql` and backfilled from `memory.ask_ledger_entries`; the newest entry the authoring saw, so a later entry triggers a re-author. No FK |

**Constraints:**
- `acceptance_criteria_maps_version_positive`: `version >= 1`
- `acceptance_criteria_maps_session_version_uniq`: UNIQUE `(session_id, version)` — also serves the newest-version lookup

---

### `memory.acceptance_criteria`

Added in `20260820000002_acceptance_criteria_maps.sql` (issue temet-ai/mike-private#152). One criterion within a map, as a row
rather than a jsonb entry because each carries its own provenance and must be individually addressable.
`operator` restates something the operator said, with the words in `operator_span`; `inferred` came from
repo context and is what an operator should look at first. Issue temet-ai/mike-private#153 requires each criterion to name the
state of the surface after the change, never the artifact added (`steerer/criteria_contract.rs` rejects
artifact-scoped criteria before they are stored).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `map_id` | `BIGINT NOT NULL` | FK → `memory.acceptance_criteria_maps(id)` ON DELETE CASCADE |
| `ord` | `INTEGER NOT NULL` | Position within the map |
| `criterion` | `TEXT NOT NULL` | |
| `provenance` | `TEXT NOT NULL` | `operator` or `inferred` |
| `operator_span` | `TEXT NULL` | The operator's own words the criterion rests on; required for `operator`, forbidden for `inferred` |

**Constraints:**
- `acceptance_criteria_provenance_check`: `provenance IN ('operator', 'inferred')`
- `acceptance_criteria_operator_span_backs_the_claim`: `(provenance = 'operator' AND operator_span IS NOT NULL AND length(trim(operator_span)) > 0) OR (provenance = 'inferred' AND operator_span IS NULL)`
- `acceptance_criteria_non_empty`: `length(trim(criterion)) > 0`
- `acceptance_criteria_map_ord_uniq`: UNIQUE `(map_id, ord)`

**Indexes:**
- `idx_acceptance_criteria_map_ord`: `(map_id, ord)` (BTree) — the join from the newest map

---



### `memory.dossier_entries`

Added in migration `20260924180000_ask_ledger_dossier_entries.sql`. Directives (`kind = 'directive'`:
`data.class`, `data.polarity`, `data.pattern`, `data.span`) extracted from genuine operator prompts,
and promises (`kind = 'promise'`: `data.span`, `data.backing`, `data.registry_coverage`,
`data.session_class`) opened for turns whose `turn_facts.ended_on = 'promise_or_wait'`. Promises are
phrase-selected and are not a complete record of waits.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `operator_turn_id` | `BIGINT NULL` | FK → `memory.operator_turns(id)` ON DELETE SET NULL |
| `kind` | `TEXT NOT NULL` | `directive` or `promise` |
| `data` | `JSONB NOT NULL` | per kind, above |
| `status` | `TEXT NOT NULL DEFAULT 'open'` | `open`, `kept`, `broken`, `withdrawn`, `superseded`; changes at most once |
| `source` | `TEXT NOT NULL` | `capture`, `judge` or `operator` |
| `source_ref` | `TEXT NOT NULL` | provenance as `<schema.table>:<id>` |
| `dedup_key` | `TEXT NOT NULL` | `request:<id>:<class>:<polarity>` or `turn:<id>`; unique with `kind` |
| `extractor_version` | `TEXT NOT NULL` | `directives-v1`, `promises-v1` |
| `effective_at` | `TIMESTAMPTZ NOT NULL` | when it happened |
| `recorded_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | when mike learned it |
| `closed_at`, `close_recorded_at` | `TIMESTAMPTZ NULL` | when it ended, when mike learned that |
| `closed_by` | `TEXT NULL` | what ended it, same pattern as `source_ref` |

As of T an entry is visible when `recorded_at <= T` and open unless `close_recorded_at <= T`
(`list_entries_as_of`). Rows are never deleted. **Indexes:** `(session_id, effective_at)`; partial
`(session_id) WHERE status = 'open'`; unique `(kind, dedup_key)`.

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `closed_at` | timestamptz |
| `close_recorded_at` | timestamptz |

### `memory.directive_proposal_evidence`

| Column | Definition |
|---|---|
| `proposal_id` | bigint NOT NULL REFERENCES memory.directive_proposals(id) ON DELETE CASCADE |
| `kind` | text NOT NULL CHECK (kind IN ('dossier_entry', 'turn_verdict', 'evaluation', 'session_end')) |
| `ref_id` | bigint NOT NULL |
| `operator_turn_id` | bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL |
| `session_id` | bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL |
| `observed_at` | timestamptz NOT NULL |
| `detail` | jsonb NOT NULL DEFAULT '{}'::jsonb |

### `memory.directive_lessons`

Added in migration `20260929000001_improvement_schema.sql` (JC21, "judgement at work"). One row
per `(directive proposal, lesson)` the JC18 J6 approval wrote — one target project can mean one
proposal produces several lessons, one row each. `project_id` is denormalized from the lesson for
a direct per-project listing without a join back through `memory.lessons`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `proposal_id` | `BIGINT NOT NULL` | FK → `memory.directive_proposals(id)` ON DELETE CASCADE |
| `lesson_id` | `BIGINT NOT NULL` | FK → `memory.lessons(id)` ON DELETE CASCADE |
| `project_id` | `BIGINT NULL` | FK → `projects.projects(id)` ON DELETE SET NULL; a proposal's decision history outlives its project, matching `directive_proposals.project_id`'s own rule |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Constraints:** `uq_directive_lessons_proposal_lesson` UNIQUE `(proposal_id, lesson_id)`.
**Indexes:** `idx_directive_lessons_proposal` on `proposal_id`; `idx_directive_lessons_lesson` on
`lesson_id`; `idx_directive_lessons_project` on `project_id WHERE NOT NULL`.

### `memory.skill_drafts`

Added in migration `20261014120000_skill_drafts.sql` (mike#613). One row per `SKILL.md` the skill
author pass (`crates/mike/src/lesson_proposer/skill_author.rs`) drafted from one topic's accepted
lessons, following the skill-creator method, with the trigger queries its evaluation needs.
Migration `20261015120000_skill_drafts_drop_unused_eval_columns.sql` dropped the `test_prompts` and
`assertions` columns the table was created with, because no evaluation step graded them. The pass runs after the topic pass on the lesson proposer's tick, behind the
`judge_lessons` toggle, and drafts the topic with the most lessons that has no draft at its current
lesson watermark (the highest lesson id in the topic). The promoter
(`scripts/learning_audit/promote.py`) ships a draft as the folder's `SKILL.md`, with
its reference files under `references/`, only once its status is `accepted` and its watermark is still
current; otherwise it writes the generated lesson list as before.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `folder` | `TEXT NOT NULL` | Skill folder: the topic, or `<topic>-<agent>` when every lesson in the topic belongs to that one agent |
| `agent_id` | `BIGINT NULL` | FK → `agents.agents(id)` ON DELETE SET NULL |
| `topic` | `TEXT NULL` | The lesson topic the draft was written for; used to find an existing skill with the same purpose (`crates/mike/src/lesson_proposer/purpose.rs`) |
| `lesson_watermark` | `BIGINT NOT NULL` | Highest lesson id in the topic when drafted |
| `lesson_ids` | `BIGINT[] NOT NULL` | The lessons the draft was written from |
| `model` | `TEXT NOT NULL` | Author model id |
| `description` | `TEXT NOT NULL` | The skill's trigger description |
| `skill_md` | `TEXT NOT NULL` | The whole `SKILL.md`, frontmatter included: a short orientation that points to every reference file with when to read it |
| `reference_files` | `JSONB NOT NULL DEFAULT '[]'` | `[{name, when_to_read, content}]`, each written to `references/<name>.md`; the detailed instructions, read only when `SKILL.md` sends the reader there |
| `trigger_queries` | `JSONB NOT NULL DEFAULT '[]'` | `[{query, should_trigger}]` for the description trigger eval |
| `status` | `TEXT NOT NULL DEFAULT 'draft'` | `draft` (written, not started), `evaluating` (the eval loop holds it and may have partial work in `eval_progress`), `accepted` or `rejected` |
| `eval` | `JSONB NULL` | The evaluation's result once decided: `evaluator_version` (which version of the evaluation method produced it), `models`, the `siblings` (other drafts of the project) and `real_skills` (`request_ids` of the captured requests whose skill list was read, `seen` skills across them, and the `skills` sampled from them) it was compared with, `trigger` (score and every query's answer), `review` (the reviewer's `decision`: `approve`, `patch`, `nothing_to_save`, `wrong_place` or `unusable` for a patch that failed the shape checks; its `reason`; the `overlap` it named, the existing skill, rule, agent or memory file that already covers the content or should own it; `belongs_in`: `rule`, `existing_skill`, `agent_persona`, `memory_file` or `nowhere`; and for a patch the `patched_description`, `patched_skill_md` and the `original_*` texts), `recheck` (the trigger test run again on a patched draft, else null), `calls` (model calls spent), `verdict` (accepted, threshold, reasons) and `findings` (missed trigger queries and wrongly triggered queries of the test that decided). A draft the loop could not evaluate holds `{evaluator_version, error, attempts}` instead |
| `eval_progress` | `JSONB NULL` | The checks done so far while `evaluating`; cleared when `eval` is stored |
| `revision` | `INTEGER NOT NULL DEFAULT 1` | Which draft this is at the topic and lesson watermark, 1 to 3. A rejected draft is redrafted with its evaluation as author input, up to three drafts per watermark |
| `generation_id` | `TEXT NULL` | |
| `cost_usd` | `DOUBLE PRECISION NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `evaluated_at` | `TIMESTAMPTZ NULL` | When `eval` was stored |

**Constraints:** UNIQUE `(project_id, folder, lesson_watermark, revision)`; CHECK `revision BETWEEN 1 AND 3`.
**Indexes:** `skill_drafts_status_idx` on `(status, created_at)`.

The eval loop (`crates/mike/src/lesson_proposer/skill_eval.rs`, migration
`20261014160000_skill_draft_eval.sql`) runs after the author pass on the same tick and call
budget, one draft per tick, `evaluating` drafts first. It is a gate of three to five model calls.
The trigger test asks the model, in batches of ten messages (twenty trigger queries are two calls,
each batch holding half of the queries that should trigger and half that should not), which skills
it would load, given the skill's name and description among the skills the project's sessions
listed and up to five other drafts of the project. The listed skills are read from the skill list
(`The following skills are available for use with the Skill tool:`) in the system prompt or
messages of the project's newest captured requests, one request per session for up to five
sessions among the newest 24 requests of 20,000 to 600,000 bytes, and sampled to 60 skills when
more are seen; `real_skills.listed` keeps all of them. One reviewer call then sees, as data in
marked blocks it must not obey, the draft `SKILL.md`, its reference files, the lessons it was
written from, every skill that already exists in the project (name and description), the agent
the lessons are scoped to and the trigger result. It answers `approve`; `patch` with a corrected
`SKILL.md`; `nothing_to_save` (already covered, naming the existing item); or `wrong_place`
(the content belongs in a rule, an existing skill, an agent persona, a memory file or nowhere,
naming the item). A draft that overlaps an existing skill by purpose is `wrong_place` with
`existing_skill`. A patched draft is checked on the trigger test once more with its new
description. A draft is accepted when the reviewer approves and its trigger score is at least
0.80, or the reviewer patches and the patched draft's trigger score is at least 0.80; an accepted
patch replaces `description` and `skill_md`, and the original text stays in `eval.review`.
Otherwise it is rejected. A draft the reviewer answers `nothing_to_save` or `wrong_place` is not
redrafted at that lesson watermark. A draft needing more calls than one tick's budget allows is
finished over several ticks. Whether a shipped skill helped is measured afterwards from live use,
in `memory.skill_live_outcomes`.

`EVALUATOR_VERSION` in `skill_eval.rs` names the evaluation method and is written into `eval` as
`evaluator_version` and into `eval_progress`. At the start of every eval pass, rejected drafts
whose `eval` carries a lower `evaluator_version` (or none) are set back to `draft` with `eval`,
`evaluated_at`, `eval_progress` and their `skill_draft_eval_failures` row cleared, so the first
pass after a deploy that raises the version evaluates them again. Saved `eval_progress` from
another version is discarded and the draft starts over.

### `memory.skill_live_outcomes`

Added in migration `20261014240000_skill_live_outcomes.sql` (mike#613). How a skill the learning
promoter shipped has done in real use: one row per project, skill folder and shipment. The
promoter (`scripts/learning_audit/promote.py`, `live.py`) measures each skill it owns in its
ledger (`~/.local/state/mike-learning-promotion/skill-installs.json`, matched by project uuid and
folder, not by name alone) on every run and posts the numbers to
`POST /api/learning/skill-outcomes`. A load is a `Skill` tool use of the folder in the project
since `window_start` (the shipment day, at most 30 days back). A loaded turn is a genuine operator
turn with a request that carried a load; it is judged when its `memory.operator_turns.reaction` is
`accepted`, `corrected` or `redirected`. The base counts are the same over all of the project's
genuine, judged operator turns in the window.

| Column | Type | Purpose |
|---|---|---|
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `folder` | `TEXT NOT NULL` | The skill folder |
| `first_shipped_on` | `DATE NOT NULL` | The ledger's `first_emitted`; a skill shipped again after a retirement is a new row |
| `window_start`, `window_end` | `TIMESTAMPTZ NOT NULL` | The measured window |
| `loads`, `load_sessions`, `loaded_turns` | `INTEGER NOT NULL` | Skill uses, the sessions they came from, the operator turns that contained one |
| `judged_turns`, `corrected_turns` | `INTEGER NOT NULL` | Of the loaded turns, those judged and those judged corrected |
| `base_judged_turns`, `base_corrected_turns` | `INTEGER NOT NULL` | The same two counts over all of the project's turns in the window |
| `retired_at`, `retired_reason` | `TIMESTAMPTZ`, `TEXT` | Set together, never cleared by a later measurement. `unloaded`: no load for 30 days. `corrected_above_base`: at least 10 loads and a corrected rate (`corrected_turns / judged_turns`) above the base rate |
| `measured_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:** primary key `(project_id, folder, first_shipped_on)`; CHECKs keep the counts
non-negative and ordered (`judged_turns <= loaded_turns`, `corrected_turns <= judged_turns`,
`base_corrected_turns <= base_judged_turns`).

A retired skill's row is part of the next skill draft's author input: while no draft that is not
rejected has been written for the folder since `retired_at`, `RETIRED SKILL` in the author input
carries these numbers and the last accepted draft's text, so the topic is rewritten instead of
repeated.

`memory.skill_evidence_v` is the per-skill read for the learning observer: one row per project and
folder with the newest draft's status and evaluation (`trigger_score`, `patched_trigger_score`,
`review_decision`, `review_reason`, `review_overlap`, `review_belongs_in`, `eval_calls`,
`eval_error`) beside the newest shipment's live numbers (`loads`, `corrected_rate`,
`base_corrected_rate`, `retired_reason`). The weekly learning audit reports the live half in
`## Live use of shipped skills`.

### `memory.skill_draft_failures`

Added in the same migration. One row per topic and lesson watermark at which the skill author
pass failed: the model call failed, the draft was rejected by its shape check, or it could not be
written. The pass waits 2^`attempts` hours after each failure and stops trying the topic after
three, until a new lesson raises its watermark, so a topic that cannot be drafted is not retried
on every tick.

| Column | Type | Purpose |
|---|---|---|
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `folder` | `TEXT NOT NULL` | As in `memory.skill_drafts` |
| `lesson_watermark` | `BIGINT NOT NULL` | The watermark the attempts were made at |
| `attempts` | `INTEGER NOT NULL DEFAULT 1` | Failed attempts at this watermark |
| `last_error` | `TEXT NOT NULL` | The last failure's reason |
| `last_failed_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Primary key:** `(project_id, folder, lesson_watermark)`.

### `memory.harvest_runs`

Added in migration `20261014140000_harvest_runs.sql` (mike#611). One row per finished model reply in
which the memory harvester (`crates/mike/src/memory_harvester/`) found at least one
`<mem:update>` tag; a reply with no tag writes no row. The weekly learning audit sums these rows
to show whether sessions use the memory verb grammar. `request_id` has no foreign key because the
retention sweep deletes old requests and this record is meant to outlive them.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL |
| `request_id` | `BIGINT NOT NULL` | The request whose reply carried the tags |
| `tags_found` | `INTEGER NOT NULL` | Tags the parser recognised |
| `lessons_written` | `INTEGER NOT NULL` | Tags stored as new lessons |
| `lessons_skipped` | `INTEGER NOT NULL` | Tags not stored: a lesson with the same text already exists, or the write failed |
| `lessons_truncated` | `INTEGER NOT NULL` | Stored bodies cut to `max_lesson_bytes` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:** `harvest_runs_created_at_idx` on `created_at`.

### `memory.skill_draft_eval_failures`

Added in migration `20261014160000_skill_draft_eval.sql`. One row per skill draft whose evaluation
failed: a model call failed or the grader returned an unusable answer. The eval loop waits
2^`attempts` hours after each failure and rejects the draft, with the last error in its `eval`,
after the third. A model refusal from the call limiter is not a failure.

| Column | Type | Purpose |
|---|---|---|
| `draft_id` | `BIGINT PRIMARY KEY` | FK → `memory.skill_drafts(id)` ON DELETE CASCADE |
| `attempts` | `INTEGER NOT NULL DEFAULT 1` | Failed attempts |
| `last_error` | `TEXT NOT NULL` | The last failure's reason |
| `last_failed_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

### `memory.skill_policies`

Added in `20261014210000_skill_review_controls.sql`, for the Skills page (`/skills`, routes under
`/api/skills` in `crates/mike/src/api/skills/`). Whether mike may write learned skills into a project's
checkouts on its own: `auto` (the promoter, `scripts/learning_audit/promote.py`, ships drafts that passed
evaluation, or a generated skill when there is none) or `ask` (it ships only drafts the user approved). A
project with no row is `ask`, so a project first seen after this table was created never receives a skill
unasked; the migration writes `auto` for every unmerged project that existed when it ran. Read and written
by `store/persistence/lessons/skill_review.rs` and `skill_review_writes.rs`.

| Column | Type | Purpose |
|---|---|---|
| `project_id` | `BIGINT PRIMARY KEY` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `mode` | `TEXT NOT NULL` | CHECK `IN ('auto', 'ask')` |
| `changed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `changed_by` | `TEXT NOT NULL` | |

---

### `memory.skill_retirements`

Added in `20261014210000_skill_review_controls.sql`. Skill folders the user retired on the Skills page. The
promoter removes them from every checkout and does not write them again until the user restores the
folder, which deletes the row. Read and written by `store/persistence/lessons/skill_review.rs` and
`skill_review_writes.rs`.

| Column | Type | Purpose |
|---|---|---|
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `folder` | `TEXT NOT NULL` | The skill folder name |
| `retired_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Primary key:** `(project_id, folder)`.

---

### `memory.skill_installs`

Added in `20261014210000_skill_review_controls.sql`. What the learning promoter last wrote into project
checkouts on each machine, reported by the promoter at the end of every run through
`POST /api/skills/installs` (`store/persistence/lessons/skill_review_writes.rs`; each report replaces that
machine's rows): the `SKILL.md` path, the draft it came from (NULL for the generated skill), when it was
first written and, once removed, when and why. `lessons/skill_coverage.sql` uses the live rows to decide
which `skill`-placed lessons a shipped skill already covers (lessons at or below the install's
`lesson_watermark` whose topic slug matches the folder). Read by the Skills page (`skill_review.rs`).

| Column | Type | Purpose |
|---|---|---|
| `machine_uuid` | `UUID NOT NULL` | No FK to `projects.machines` |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `folder` | `TEXT NOT NULL` | The skill folder name |
| `machine_name` | `TEXT NOT NULL` | |
| `skill_path` | `TEXT NOT NULL` | The `SKILL.md` path on that machine |
| `draft_id` | `BIGINT NULL` | FK → `memory.skill_drafts(id)` ON DELETE SET NULL; NULL for the generated skill |
| `lesson_watermark` | `BIGINT NOT NULL` | The highest lesson id the shipped skill was built from |
| `first_written` | `DATE NOT NULL` | |
| `retired_at` | `DATE NULL` | When the promoter removed it |
| `retired_reason` | `TEXT NULL` | CHECK `IN ('unloaded', 'corrected_above_base', 'user', 'policy')` (`corrected_above_base` added by `20261015090000_skill_install_corrected_reason.sql`): `unloaded` no load in 30 days; `corrected_above_base` at least 10 loads and the turns that loaded it were corrected more often than the project's turns over the same window; `user` retired on the Skills page; `policy` the project asks first and the user has not approved it |
| `reported_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Primary key:** `(machine_uuid, project_id, folder)`.

**Indexes:**
- `skill_installs_project_idx`: `(project_id, folder)` (BTree)

---




### The user's decisions on rule and agent proposals

Added in migration `20261016100000_lesson_placement_reviews.sql`, for the Proposals page
(`/proposals`, routes under `/api/proposals` in `crates/mike/src/api/proposals.rs`).

`memory.lesson_placement_reviews`: one row per lesson placed as `rule` or `agent` that the user has
decided, keyed `lesson_id` (cascades with the lesson): `decision` (`accepted` or `declined`) and
`decided_at`. A lesson with no row is undecided; undoing a decision deletes the row. Nothing writes
an accepted proposal into a rule or agent file yet; the page shows the file and the text to add.

### The user's control over learned skills

Added in migration `20261014210000_skill_review_controls.sql`, for the Skills page (`/skills`,
routes under `/api/skills` in `crates/mike/src/api/skills/`). The learning promoter
(`scripts/learning_audit/promote.py`) reads the first three and writes the fourth through
`POST /api/skills/installs`.

`memory.skill_drafts` gains four columns: `review` (`approved` or `rejected`, the user's decision,
NULL when undecided), `reviewed_at`, `edited_at`, and `authored_skill_md` (the author pass's text,
kept the first time the user edits `skill_md`).

`memory.skill_policies`: one row per project, `mode` `auto` (ship drafts that passed review, or a
generated skill when there is none) or `ask` (ship only drafts the user approved), with
`changed_at` and `changed_by`. A project with no row is `ask`. The migration writes `auto` for
every project that existed when it ran.

`memory.skill_retirements`: `(project_id, folder)` the user retired, with `retired_at`. The
promoter removes the folder from every checkout and does not write it until the row is deleted.

`memory.skill_installs`: what the promoter last wrote on each machine, keyed
`(machine_uuid, project_id, folder)`: `machine_name`, `skill_path`, `draft_id` (NULL for the
generated skill), `lesson_watermark`, `first_written`, and once removed `retired_at` and
`retired_reason` (`unloaded`, `corrected_above_base`, `user` or `policy`). Each report replaces
that machine's rows.

The Skills page reads each folder's newest draft (its trigger score, the re-checked score after a
reviewer rewrite, and the reviewer's decision, reason, named overlap and place from `eval.review`)
beside the newest row of `memory.skill_live_outcomes` for that folder, and the project list shows
each project's live skill count against the `lessons.skills.max_per_project` setting.
Index `skill_installs_project_idx` on `(project_id, folder)`.

### `memory.lesson_proposal_evidence`

Added in migration `20260929000001_improvement_schema.sql` (JC21). One row per
`memory.turn_verdicts` row the lesson proposer (`crates/mike/src/lesson_proposer/`) has examined,
whether or not it produced or reinforced a lesson. `verdict_id` is `UNIQUE` — it is both the
anti-join key the candidate-selection query in `store/persistence/lessons/proposals.rs` uses to
skip an already-examined verdict (`rust-database-reliability.md` allows this: the uniqueness lives on a real
surrogate FK, never on a sha), and the invariant that a verdict is examined at most once.
`lesson_id` is `NULL` when the proposer looked at the verdict and wrote no lesson (confidence
below threshold, no resolved correcting message, or the model's own `rule = null`); set when it
created a new raw lesson or reinforced an existing one (`consolidation_count` on that lesson
incremented in the same statement). `similar_corrections` holds the up-to-three earlier
corrections (`semantic::Retriever::neighbours`, kind `Correction`) the proposer read before
deciding, with their existing lessons if any, exactly as read — genuinely schemaless per-call
context, not a domain object array. Named `similar_corrections`, not `similar`: `SIMILAR` is a
reserved PostgreSQL keyword (the `SIMILAR TO` operator) and cannot be used unquoted as a column
identifier, in `CREATE TABLE` or in an `INSERT` column list.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `lesson_id` | `BIGINT NULL` | FK → `memory.lessons(id)` ON DELETE CASCADE; NULL = examined, no lesson |
| `verdict_id` | `BIGINT NOT NULL UNIQUE` | FK → `memory.turn_verdicts(id)` ON DELETE CASCADE; the anti-join key and one-attempt-per-verdict invariant |
| `operator_turn_id` | `BIGINT NOT NULL` | FK → `memory.operator_turns(id)` ON DELETE CASCADE |
| `similar_corrections` | `JSONB NOT NULL DEFAULT '[]'` | Up to three earlier similar corrections read before deciding, with their existing lessons if any |
| `model` | `TEXT NOT NULL` | Proposer model id |
| `cost_usd` | `DOUBLE PRECISION NULL` | |
| `generation_id` | `TEXT NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:** `idx_lesson_proposal_evidence_lesson` on `lesson_id WHERE NOT NULL`;
`idx_lesson_proposal_evidence_turn` on `operator_turn_id`.

### `memory.lesson_proposal_jobs`

Added in `20261005000000_learning_proposal_queue.sql`. One durable replay job per verdict for the lesson
proposer (`crates/mike/src/lesson_proposer/`, queue code in
`store/persistence/lessons/proposals/queue.rs`): when a proposal has been produced but cannot be written
because embedding is unavailable, the immutable proposer inputs and the wire response are saved here as
`awaiting_embedding` and replayed once `next_attempt_at` is due (back-off `15 * 2^n` s capped at 1 h,
`retry_count` capped at 16), ending `applied`; a job whose verdict is no longer in
`memory.lesson_eligible_corrections_v` becomes `rejected`. JSONB holds replay DTOs, not filter keys; the
lifecycle and budget fields are typed.

| Column | Type | Purpose |
|---|---|---|
| `verdict_id` | `BIGINT PRIMARY KEY` | FK → `memory.turn_verdicts(id)` ON DELETE CASCADE |
| `candidate` | `JSONB NOT NULL` | The `CorrectionCandidate` the proposer was given |
| `proposed` | `JSONB NOT NULL` | The `ProposedLesson` the model returned |
| `similar` | `JSONB NOT NULL` | The similar lessons shown to the model |
| `model` | `TEXT NOT NULL` | |
| `cost_usd` | `DOUBLE PRECISION NULL` | NULL when the provider reported no usage |
| `generation_id` | `TEXT NULL` | The provider's generation id; NULL when absent |
| `retry_count` | `INTEGER NOT NULL DEFAULT 0` | CHECK `>= 0` |
| `next_attempt_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `status` | `TEXT NOT NULL` | CHECK `IN ('awaiting_embedding', 'rejected', 'applied')` |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_lesson_proposal_jobs_pending`: `(updated_at, verdict_id) WHERE status = 'awaiting_embedding'` (BTree partial)

---

### `memory.lesson_proposal_recovery`

Added in `20261005000000_learning_proposal_queue.sql`. Verdicts whose earlier proposal attempt failed on an
ambiguous dependency error and are reassessed once under the normal daily budget. The migration seeds a
row from every `memory.lesson_proposal_evidence` row with no `lesson_id` and a model other than `none`
created on or after `2026-10-01T13:02:13Z`, keeping the full evidence row as an immutable audit payload
before the current row is replaced; it makes no claim that every such row represented a valid rule. The
queue (`store/persistence/lessons/proposals/queue.rs`) marks a row `obsolete` when its verdict is no longer
in `memory.lesson_eligible_corrections_v`.

| Column | Type | Purpose |
|---|---|---|
| `verdict_id` | `BIGINT PRIMARY KEY` | FK → `memory.turn_verdicts(id)` ON DELETE CASCADE |
| `original_evidence` | `JSONB NOT NULL` | The full historical `lesson_proposal_evidence` row |
| `recovery_state` | `TEXT NOT NULL DEFAULT 'needs_reproposal'` | CHECK `IN ('needs_reproposal', 'reproposed', 'obsolete')` |
| `disposition_reason` | `TEXT NOT NULL DEFAULT ''` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

---



### `memory.lesson_proposal_guidance`

Added in migration `20261014170000_lesson_proposal_guidance.sql`. One row per correction and per
Claude Code memory file or project skill the lesson proposer was shown beside it (see
[PROJECT_KNOWLEDGE.md](PROJECT_KNOWLEDGE.md)). Not a foreign key to the verdict, so the record
outlives a pruned verdict.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `verdict_id` | `BIGINT NOT NULL` | The turn verdict the proposer examined |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` |
| `reference` | `TEXT NOT NULL` | Knowledge reference of the revision shown (`artifact:<revision>`); `UNIQUE (verdict_id, reference)` |
| `source_kind` | `TEXT NOT NULL` | `memory`, `skill`, `rule` or `agent` (the last two added in `20261014200000_lesson_placement.sql`; their `reference` is `file:<path>`) |
| `name` | `TEXT NOT NULL` | Frontmatter name, else the title |
| `relative_path` | `TEXT NOT NULL` | Path within the memory or skills directory, or `.claude/rules/…` / `.claude/agents/…` for a rule or agent |
| `similarity` | `DOUBLE PRECISION NULL` | Embedding similarity; NULL when the document matched on text |
| `cited` | `BOOLEAN NOT NULL` | The proposer's reason names the path, the file name without extension, or the skill name |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:** `lesson_proposal_guidance_project_created` on `(project_id, created_at)`.

When consolidation applies a confirmed `supersedes` or write-time `refines` relation, the evidence
rows of the archived lesson are re-pointed at the replacement (`lesson_id` update), so the
replacement is backed by the union of the corrections behind both.

### `memory.lesson_shape_findings`

Added in migration `20261014190000_lesson_shape_findings.sql`. One row per lesson that has been
checked for rule shape (`crates/mike/src/lesson_shape.rs`): a durable instruction in the
imperative with its trigger and what to do, not a status report about a session. The check runs
at write time in `POST /api/lessons` (the `record_lesson` tool), in the lesson proposer when it
creates a lesson, and as a scan over lessons that have no row yet on the proposer tick
(`crates/mike/src/lesson_proposer/shape_pass.rs`).

A lesson with an unresolved non-rule row (`resolution IS NULL`) is excluded from session
consolidation to daily (`claim_raw_lessons_for_consolidation`, `discover_idle_projects`), from
the proposer's auto-promotion, and, while it is still `raw`, from injection
(`list_injectable_lessons`). The shape pass rewrites it into one to four rules
(`resolution = 'rewritten'`: each rule is a new lesson at the same stage with a confirmed
`supersedes` relation of method `rewritten` to the original, and the original is archived as
`superseded` with its text kept), moves a status note out of lessons (`resolution =
'not_a_lesson'`: archived as `not_a_lesson`, and a `handover` note written to its session when it
has one), or gives up after three failed attempts (`resolution = 'kept'`). `kept` is also written
at once when the lesson is recorded while the `judge_lessons` toggle is off, and by the nightly run
for a hold that has been open for 14 days.

| Column | Type | Purpose |
|---|---|---|
| `lesson_id` | `BIGINT PRIMARY KEY` | FK → `memory.lessons(id)` ON DELETE CASCADE |
| `shape` | `TEXT NOT NULL` | CHECK `rule \| status \| over_length` |
| `detail` | `TEXT NOT NULL` | What the check found, with the offending input and its character position |
| `chars` | `INTEGER NOT NULL` | Length of the text in characters when checked |
| `found_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `attempts` | `INTEGER NOT NULL DEFAULT 0` | Failed rewrite attempts; the pass waits 2^attempts hours after each |
| `last_error` | `TEXT NULL` | |
| `last_attempt_at` | `TIMESTAMPTZ NULL` | |
| `resolution` | `TEXT NULL` | CHECK `none_needed \| rewritten \| not_a_lesson \| kept`; NULL = open |
| `resolved_at` | `TIMESTAMPTZ NULL` | Set exactly when `resolution` is |
| `replacement_lesson_id` | `BIGINT NULL` | FK → `memory.lessons(id)` ON DELETE SET NULL; the first rule a rewrite produced |

**Constraints:** `lesson_shape_findings_resolution_consistent` (`resolution` and `resolved_at` are
both NULL or both set); `lesson_shape_findings_rule_needs_nothing` (`shape = 'rule'` exactly when
`resolution = 'none_needed'`). **Indexes:** `idx_lesson_shape_findings_unresolved` on
`(found_at, lesson_id) WHERE resolution IS NULL`.

### `memory.lesson_placements`

Added in migration `20261014200000_lesson_placement.sql`. The homes a learned lesson can have, one
row each: `rule` (a short constraint tied to files, proposed to the operator and never written by
mike), `skill` (a procedure for a recognisable situation, the only home that feeds skill drafts),
`agent` (who an agent is and how it decides, proposed to the operator and never written by mike),
`memory` (a fact or correction recalled when relevant) and `nowhere` (already covered, or not worth
keeping).

| Column | Type | Purpose |
|---|---|---|
| `id` | `SMALLINT PRIMARY KEY` | 1 rule, 2 skill, 3 agent, 4 memory, 5 nowhere |
| `code` | `TEXT NOT NULL UNIQUE` | |
| `description` | `TEXT NOT NULL` | |

### `memory.lesson_placement_failures`

Added in migration `20261014200000_lesson_placement.sql`. Failed placement attempts for one lesson
(the model call failed or its answer could not be stored). The placement pass
(`crates/mike/src/lesson_proposer/placement_pass.rs`) waits 2^attempts hours after each failure and
stops trying the lesson after the third; the weekly learning audit counts it as stuck.

| Column | Type | Purpose |
|---|---|---|
| `lesson_id` | `BIGINT PRIMARY KEY` | FK → `memory.lessons(id)` ON DELETE CASCADE |
| `attempts` | `INTEGER NOT NULL DEFAULT 1` | |
| `last_error` | `TEXT NOT NULL` | |
| `last_failed_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

### `memory.lesson_placement_reviews`

Added in `20261016100000_lesson_placement_reviews.sql`, for the Proposals page (`/proposals`, routes under
`/api/proposals` in `crates/mike/src/api/proposals.rs`). The user's decision on a lesson placed as a rule or
an agent statement: `accepted` (the user wants it in the rule or agent file the page names) or `declined`.
A lesson with no row has not been decided; clearing a decision deletes the row. Nothing writes an accepted
proposal into a rule or agent file; the page shows the file and the text to add. Read and written by
`store/persistence/lessons/placement_proposals.rs`.

| Column | Type | Purpose |
|---|---|---|
| `lesson_id` | `BIGINT PRIMARY KEY` | FK → `memory.lessons(id)` ON DELETE CASCADE |
| `decision` | `TEXT NOT NULL` | CHECK `IN ('accepted', 'declined')` |
| `decided_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Refreshed when the decision changes |

---


### `memory.lesson_proposer_daily_budget` maintenance counter

`maintenance_consumed BIGINT NOT NULL DEFAULT 0` was added in migration
`20261014230000_lesson_maintenance_budget.sql`. It counts the model calls the shape rewrite pass and
the placement pass reserved on the UTC day in `day`, apart from `consumed`, which counts the
correction proposer's calls. The ceiling for `maintenance_consumed` is the setting
`judge_lessons.maintenance_max_per_day`.

### `memory.lesson_maintenance_refusals`

Added in migration `20261014230000_lesson_maintenance_budget.sql`. The latest refusal of a model call
for each lesson maintenance pass, overwritten on every refusal. `refused_at` feeds the
`llm_role_health` capability row on the home page; `logged_at` limits the matching info log line to
once an hour per pass.

| Column | Type | Purpose |
|---|---|---|
| `pass` | `TEXT PRIMARY KEY` | `shape` or `placement` |
| `reason` | `TEXT NOT NULL` | `daily_budget` (the maintenance count reached its ceiling) or `provider_limit` (the call limiter refused the call) |
| `cap` | `BIGINT NOT NULL` | The daily ceiling in force at the refusal |
| `refused_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `logged_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When the refusal was last written to the log |

### `memory.lesson_kinds`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `code` | text NOT NULL UNIQUE |

### `memory.lesson_sources`

Lookup for `memory.lessons.source_id`: who recorded the lesson. Rows 1–3 seeded by the baseline;
`harvester` added by `20260613000001_memory_recall_schema.sql`, `directive` and `judge` by
`20260929000001_improvement_schema.sql`, `carry` by `20261006100000_fleet_run_records.sql` (sequence-assigned,
with `memory.carried_lessons`). Joined wherever lessons are listed (`lessons/reads.rs`, `injectable.rs`,
`improvement/lesson_actions*.sql`, `memory.portable_lesson_v`).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `agent`(1) `operator`(2) `copilot`(3) `harvester`(4) `directive`(5) `judge`(6) `carry`(7).

---

### `memory.lesson_stages`

Lookup for `memory.lessons.stage_id` and for `memory.consolidation_actions.before_stage_id` /
`after_stage_id`. Seeded by the baseline. `memory.lessons` enforces `(stage_id = 4) = (archived_at IS NOT NULL)`;
only `daily` and `long_term` lessons are injectable.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `raw`(1) `daily`(2) `long_term`(3) `archived`(4).

---

### `memory.lesson_archived_reasons`

Lookup for `memory.lessons.archived_reason_id`. Rows 1–5 seeded by the baseline; `rejected_by_operator`
added by `20260929000001_improvement_schema.sql`, `not_a_lesson` by `20261014190000_lesson_shape_findings.sql`
(used by the shape pass, `lessons/shape_apply.rs`). `memory.lessons` requires `archived_reason_id = 1`
(`duplicate`) whenever `duplicate_of_id` is set.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | |

Seeded rows: `duplicate`(1) `expired`(2) `trimmed`(3) `superseded`(4) `operator`(5)
`rejected_by_operator`(6) `not_a_lesson`(7).

---




### `memory.pattern_assignments`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `corrected_turn_id` | bigint NOT NULL |
| `correction_item_id` | bigint NOT NULL REFERENCES memory.semantic_items(id) ON DELETE CASCADE |
| `qualification` | text NOT NULL |
| `qualification_ref` | text NOT NULL |
| `pattern` | text |
| `score` | double precision NOT NULL |
| `similarity` | double precision NOT NULL |
| `anchor_item_id` | bigint REFERENCES memory.semantic_items(id) ON DELETE SET NULL |
| `runner_up_pattern` | text |
| `margin` | double precision |
| `top_anchors` | jsonb NOT NULL |
| `floor_id` | bigint NOT NULL REFERENCES memory.semantic_floors(id) |
| `tagger_version` | text NOT NULL |
| `embedder_key` | text NOT NULL |
| `assigned_at` | timestamptz NOT NULL DEFAULT now() |

### `memory.pattern_candidates`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `status` | text NOT NULL DEFAULT 'open' |
| `cluster_version` | text NOT NULL |
| `embedder_key` | text NOT NULL |
| `floor_id` | bigint REFERENCES memory.semantic_floors(id) ON DELETE SET NULL |
| `window_start` | timestamptz NOT NULL |
| `window_end` | timestamptz NOT NULL |
| `member_item_ids` | bigint[] NOT NULL |
| `member_count` | integer NOT NULL |
| `session_count` | integer NOT NULL |
| `medoid_item_id` | bigint |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `decided_at` | timestamptz |
| `decision_note` | text |
| `named_pattern` | text |

### `memory.semantic_embedders`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `embedder_key` | text NOT NULL |
| `model` | text NOT NULL |
| `dim` | integer NOT NULL CHECK (dim > 0) |
| `chunk_max_chars` | integer NOT NULL CHECK (chunk_max_chars > 0) |
| `prefix_version` | text NOT NULL |
| `urls_seen` | text[] NOT NULL |
| `canary_version` | text NOT NULL |
| `canary_vectors` | jsonb NOT NULL |
| `status` | text NOT NULL DEFAULT 'current' |
| `first_seen_at` | timestamptz NOT NULL DEFAULT now() |
| `last_checked_at` | timestamptz NOT NULL DEFAULT now() |
| `copy_valid_from` | timestamptz |
| `copy_verify` | jsonb |

### `memory.semantic_floors`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `embedder_key` | text NOT NULL |
| `pair` | text NOT NULL |
| `quantile` | double precision NOT NULL CHECK (quantile > 0.5 AND quantile < 1) |
| `floor` | double precision NOT NULL |
| `n_pairs` | bigint NOT NULL CHECK (n_pairs > 0) |
| `p50` | double precision NOT NULL |
| `p95` | double precision NOT NULL |
| `p99` | double precision NOT NULL |
| `p999` | double precision NOT NULL |
| `sample` | jsonb NOT NULL |
| `builder_version` | text NOT NULL |
| `status` | text NOT NULL |
| `promoted_from_id` | bigint REFERENCES memory.semantic_floors(id) ON DELETE SET NULL |
| `computed_at` | timestamptz NOT NULL DEFAULT now() |

### `memory.semantic_items`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `kind` | text NOT NULL |
| `session_id` | bigint REFERENCES sessions.sessions(id) ON DELETE CASCADE |
| `operator_turn_id` | bigint REFERENCES memory.operator_turns(id) ON DELETE CASCADE |
| `project_id` | bigint |
| `request_id` | bigint |
| `origin_session_uuid` | uuid |
| `origin_turn_id` | bigint |
| `source_ref` | text NOT NULL |
| `pattern` | text |
| `text` | text NOT NULL |
| `text_chars` | integer NOT NULL |
| `embedding` | vector(768) NOT NULL |
| `embedder_key` | text NOT NULL REFERENCES memory.semantic_embedders(embedder_key) |
| `vector_source` | text NOT NULL |
| `builder_version` | text NOT NULL |
| `effective_at` | timestamptz NOT NULL |
| `recorded_at` | timestamptz NOT NULL DEFAULT now() |
| `data` | jsonb NOT NULL DEFAULT '{}'::jsonb |

### `memory.turn_outcomes`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `code` | text NOT NULL UNIQUE |

## Domain: `prompt_kinds`

Added in migration `20260620000001_prompt_kind.sql`. Classifies each operator turn's opening prompt into one of
eight kinds. Migration `20260722020000_prompt_kind_harness_families.sql` added the four harness-generated kinds
(`session-recap`, `background-event`, `compaction`, `analysis-probe`) and reclassified rows previously filed
under the `operator` catch-all whose `user_prompt` matched one of those families. Migration
`20260722400000_teammate_message_agent_wrapper.sql` extended `teammate-message` to also cover the
`<agent-message from="...">` subagent-report wrapper, alongside the `<teammate-message teammate_id="...">`
peer-session wrapper — both carry the same provenance (another agent wrote this, not the operator).

### `prompt_kinds.kinds` (vocab)

| `id` | `code` | Matches (LIKE on `user_prompt`) |
|---|---|---|
| 1 | `activity-label` | `Describe your most recent action in 3-5 words` |
| 2 | `hook-feedback` | `Stop hook feedback:` |
| 3 | `teammate-message` | `<teammate-message` OR `<agent-message from=` |
| 4 | `operator` | catch-all fallback |
| 5 | `session-recap` | `The user stepped away and is coming back` |
| 6 | `background-event` | `[SYSTEM NOTIFICATION - NOT USER INPUT]` |
| 7 | `compaction` | `This session is being continued from a previous conversation` OR `Summarize this portion of a Claude Code session transcript` |
| 8 | `analysis-probe` | `Analyze this Claude Code session and extract structured facets` OR `has the following stopping condition been satisfied` |

Classification is first-match-wins in id order 1, 2, 3, 5, 6, 7, 8, then `operator` (4) as the catch-all fallback
— the CASE order used by both the migration and the capture-side Rust INSERT
(historical `operator_turns.rs::write_prompt_kind_classification`).

Migration `20260924100000_prompt_classification_unified.sql` adds ids 10-17: `loop-wakeup`, `channel-wake`,
`task-notification`, `tool-loaded`, `mailbox-delivery`, `steering-payload`, `harness-authored`, `agent-lineage`.
Capture now classifies with `dossier::genuine::classify_prompt` (rule `a` templates, rule `c` agent id or
harness author, rule `envelope` from the unstripped last user message), not with a CASE over LIKE patterns;
the table above describes the historical matching only.

### `prompt_kinds.classifications`

One row per primary-lane `user_prompt` request that carries a prompt (grain: one row per `request_id`), written at capture whether or not the request opens an operator turn. Closed turns were backfilled by the migration. `rule` (`a`, `c`, `envelope`), `repeat_key` and `classifier_version` are NULL on rows written before migration `20260924100000`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `request_id` | `BIGINT NOT NULL` | FK → `requests.requests(id)` ON DELETE CASCADE; the turn's opening request |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE; denormalized for session-scoped queries |
| `kind_id` | `BIGINT NOT NULL` | FK → `prompt_kinds.kinds(id)` |
| `classified_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_pk_classifications_request_id`: `request_id` (BTree)
- `idx_pk_classifications_session_id`: `session_id` (BTree)
- `idx_pk_classifications_kind_id`: `kind_id` (BTree)
- `idx_pk_classifications_session_kind`: `(session_id, kind_id)` (BTree) — primary read path
- `uq_pk_classifications_request_id`: UNIQUE `request_id` — one classification per opening request

---

## Domain: `session_judges`

Added in migration `20260625000002_session_judge.sql` (GitHub issue temet-ai/mike-private#24). Per-session LLM-judge outcome and
quality score, distinct from `memory.operator_turns.judge_model`/`judge_rubric_version` above (that pair
labels one turn; this domain scores a whole session).

### `session_judges.outcomes` (vocab)

Seeded: `unknown` (1), `success` (2), `partial` (3), `failure` (4). `unknown` is id=1 — the safe default.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Outcome code |

### `session_judges.verdicts`

Grain: one row per judge run per session. Re-runs under the same `(session_id, judge_model, rubric_version)`
**append** a new row rather than overwriting — the full judge history is preserved for auditing, and
`mike.session_quality_v` (below) surfaces only the latest per session via `DISTINCT ON`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `outcome_id` | `BIGINT NOT NULL` | FK → `session_judges.outcomes(id)` |
| `quality_score` | `NUMERIC(4,3) NULL` | CHECK NULL or `0 <= score <= 1`; NULL when the judge produced no numeric score |
| `rationale` | `TEXT NULL` | Free-text justification from the judge; full text, never truncated |
| `judge_model` | `TEXT NOT NULL` | OpenRouter model id that produced this verdict |
| `rubric_version` | `INTEGER NOT NULL` | CHECK `>= 1` |
| `judged_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `mode` | `TEXT NOT NULL DEFAULT 'live'` | `live` (the production judge; every row before migration `20260925000500`), `shadow` or `eval` (a `mike eval run --role session_judge` item). Every reader of "the session's verdict" filters `mode = 'live'`; `tests/schema/session_verdict_readers.rs` keeps it so |
| `end_turn_id` | `BIGINT NULL` | The ending turn (the last settled genuine operator turn) the end label applies to; no FK |
| `end_label` | `TEXT NULL` | `done`, `closed_in_frustration`, `cut_off_neutral`, `fleet_seat` or `unclear`; an operator gold `session_end` label in `eval.labels` wins at read time |
| `end_source`, `end_rule`, `end_confidence`, `end_evidence` | `TEXT NULL` | `llm` or `deterministic`; the rule (`fleet-seat-v1`, `done-v1`) that decided it; the confidence; the full evidence quote |
| `failure`, `failure_tags`, `severity` | `TEXT`, `TEXT[]`, `TEXT` | What went wrong, up to three tags from the labelling-rubric vocabulary, and how badly |
| `rubric_id`, `slices`, `as_of`, `digest_version`, `input_text`, `llm_output` | | The v3 judge's rubric, the dossier render it read (whole) and its raw output |
| `generation_id`, `cost_usd`, `prompt_tokens`, `completion_tokens`, `latency_ms` | | The call's usage |

A closed-in-frustration end label on a settled ending turn makes `project_turn_outcomes` write that turn
as `abandoned` / `outcome_source = 'frustration_close'` (the only place `abandoned` is written), and
`COUNTS_AS_CORRECTION_SQL` (`operator_turns/correction.rs`) counts it as a correction in
`list_corrected_turns`, `recall_project_context` and the outcome summary.

**Indexes:**
- `idx_sj_verdicts_session_id`: `session_id` (BTree)
- `idx_sj_verdicts_outcome_id`: `outcome_id` (BTree)
- `idx_sj_verdicts_session_judged_at`: `(session_id, judged_at DESC)` (BTree) — primary read path, feeds `session_quality_v`
- `idx_sj_verdicts_session_model_rubric`: `(session_id, judge_model, rubric_version)` (BTree, not unique) — "already judged under this model+rubric?" probe; re-runs must append, not collide

### `session_judges.attempts`

Added in migration `20260921000002_session_judge_attempts.sql`. Grain: one row per
`(session_id, judge_model, rubric_version)` the session judge has called the LLM for, whether or not the
call produced a verdict. `session_judges.verdicts` cannot answer "did we already try?" because an
`unknown` outcome writes no overview and a repeat attempt appends no row, so before this table a session
with an `unknown` verdict and a NULL overview was re-selected and re-judged on every poll (about 16,000
calls in a week for 143 verdict rows). `fetch_sessions_needing_judgment` now selects a session only when
it has no attempt row, has operator turns newer than `last_attempt_at`, or its last attempt failed and
the backoff has elapsed.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE; leads the primary key, so it is also the FK index |
| `judge_model` | `TEXT NOT NULL` | OpenRouter model id the attempt used |
| `rubric_version` | `INTEGER NOT NULL` | CHECK `>= 1` |
| `last_attempt_at` | `TIMESTAMPTZ NOT NULL` | When the latest attempt began, captured before the digest is read so an operator turn landing mid-call still compares as newer |
| `attempt_count` | `INTEGER NOT NULL DEFAULT 1` | CHECK `>= 1`; every attempt, successful or not |
| `consecutive_failures` | `INTEGER NOT NULL DEFAULT 0` | CHECK `>= 0`; attempts in a row that ended without what the judge owed (LLM error, breaker trip, failed verdict insert); reset by the next clean attempt; drives the backoff (5 minutes doubling, capped at 24 hours) |
| `last_digest_sha256` | `BYTEA NULL` | SHA-256 of the system prompt, a NUL separator, and the final user digest (including related-session context), scoped to this session/model/rubric. NULL until a successful judgment is recorded after the column was added. |

**Primary key:** `(session_id, judge_model, rubric_version)`. A refused call (budget or rate limit) records no
attempt, because the refusal is not the session's fault.

Migration `20260925010000_session_judge_attempt_digest.sql` adds repeat detection. Before the
LLM call, `skip_repeated_digest` compares the hash with the last successfully judged input.
A match advances only `last_attempt_at` (never backwards), skips the LLM, and leaves attempt
and failure counts unchanged. `record_judged_digest` records the hash only after the verdict
handling succeeds. A failed repeat check logs a warning and proceeds with judging; this is
not a global cache or a guarantee that every repeated call is suppressed.

**Seed:** the migration inserts one row for every `(session, model, rubric)` whose latest verdict is
`unknown`, with `last_attempt_at` set to that verdict's `judged_at`. Sessions with a known verdict are not
seeded, so an owed overview backfill still happens once.

---

## Domain: `llm`

Added in migration `20260829000002_llm_calls.sql`
(`artifacts/2026-08-29/llm-cost-and-signal-plan.md`, "Recording what calls
cost"). Records one row per outbound OpenRouter call from every LLM caller in
the process (the steerer fleet, the session judge, the turn-judge worker, the
lesson-relations proposer) — cost, tokens, and the full request body, so a
call can be read back off the live system after the fact instead of only
being visible on the wire at call time.

### `llm.calls`

Grain: one row per outbound OpenRouter call, including refused and failed
attempts, not only successes — a refusal or a truncation still explains part
of "why did the calls stop" and is worth keeping. No FK on `session_id` /
`steerer_id` / `operator_turn_id`: a call can be recorded before or after the
row it refers to exists, and a foreign key here would let a recording failure
fail the call it is trying to record, which this table's write path never
allows.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `role` | `TEXT NOT NULL` | CHECK `steerer \| session_judge \| judge_worker \| lesson_relations \| eval \| lesson_proposer`. `eval` added by `20260925000000_llm_calls_role_eval.sql`; `lesson_proposer` (the JC21 lesson proposer, `crates/mike/src/lesson_proposer/`) by `20260929000001_improvement_schema.sql`, alongside the matching widening of `llm.daily_spend.role` below (its `AFTER INSERT` trigger copies `NEW.role` verbatim, so a role `llm.calls` accepts but `llm.daily_spend` does not fails every insert of that role, not just a read) |
| `model` | `TEXT NOT NULL` | OpenRouter model id |
| `generation_id` | `TEXT NULL` | OpenRouter response `id`; a cross-reference fingerprint only, deliberately not indexed |
| `prompt_tokens` | `INTEGER NULL` | |
| `completion_tokens` | `INTEGER NULL` | |
| `reasoning_tokens` | `INTEGER NULL` | `usage.completion_tokens_details.reasoning_tokens` |
| `cached_tokens` | `INTEGER NULL` | `usage.prompt_tokens_details.cached_tokens` |
| `cost_usd` | `DOUBLE PRECISION NULL` | `usage.cost` |
| `latency_ms` | `INTEGER NULL` | |
| `outcome` | `TEXT NOT NULL` | CHECK `ok \| http_error \| timeout \| truncated \| transport_error \| refused_ceiling \| refused_rate \| refused_spend`. `transport_error` (since 2026-10-13) is a call that never got an HTTP status: connection failures, and every failure of the local `codex` transport |
| `http_status` | `INTEGER NULL` | |
| `session_id` | `BIGINT NULL` | No FK — see grain note above |
| `steerer_id` | `BIGINT NULL` | No FK — see grain note above |
| `operator_turn_id` | `BIGINT NULL` | No FK — see grain note above |
| `request_body` | `JSONB NULL` | Added in `20260902000001_llm_calls_request_body.sql`. The full outbound request body (system + messages + parameters) exactly as sent, captured once by `crates/mike/src/llm/openrouter.rs` and stored unchanged — never a slice or a summary. `NULL` on rows predating this column and on refused calls, which never build a body in the first place. This is what lets the session judge's related-sessions block (the neighbour-session context spliced into the judge prompt) be read back off the live system after the fact. |
| `machine_uuid` | `UUID NULL` | Added in `20261014130000_llm_worker_leases.sql`. The `machine_uuid` from `machine.json` of the proxy that made the call, filled by `PersistHandle::insert_llm_call`. `NULL` on rows recorded before the column existed and on rows from a proxy with no machine identity file. Not indexed: the per-machine audit reads a bounded time window through `idx_llm_calls_created_at`. |

**Indexes:**
- `idx_llm_calls_created_at`: `created_at` (BRIN) — append-only time series
- `idx_llm_calls_role_created_at`: `(role, created_at)` (BTree) — backs the per-role spend rollup behind `GET /api/llm/spend`

**Read path:** `GET /api/llm/spend` (`crates/mike/src/api/handlers.rs::get_llm_spend`) serves the three aggregate rollups (`llm_spend_by_day` / `_by_model` / `_by_role`) — cost, call count, and token totals grouped by day/model/role, never per-row bodies. There is no per-call listing route; reading one call's `request_body` (or any other single-row field) is a direct SQL read against `llm.calls`, e.g. with `psql`.

### `llm.daily_spend`

Added in migration `20260928110000_llm_daily_spend_rollup.sql`. A per-`(day, role, steerer_id, model)`
rollup of `llm.calls`, kept exact by an `AFTER INSERT` trigger (`trg_llm_calls_daily_spend`) rather
than computed at read time: `llm_spend_by_day_caller` measured at 45ms for a 30-day window over
~65k rows on a plain `SeqScan` that grows linearly with table size, already over the 50ms `read`
tier budget in `.claude/rules/rust-database-reliability.md` for a 90-day window. Every
`GET /api/llm/spend` and `GET /api/usage/openrouter` read now scans this table (at most a few
dozen rows per day) instead of `llm.calls` directly. `day` is the UTC calendar day of
`llm.calls.created_at`. `steerer_id` carries no foreign key, for the same reason `llm.calls.steerer_id`
has none — a call can be recorded, and this rollup must still update, whether or not the steerer
row it names exists right now.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `day` | `DATE NOT NULL` | UTC calendar day |
| `role` | `TEXT NOT NULL` | Same CHECK set as `llm.calls.role`, kept in lockstep by the migration that widens either |
| `steerer_id` | `BIGINT NULL` | No FK — see above |
| `model` | `TEXT NOT NULL` | |
| `call_count` | `BIGINT NOT NULL DEFAULT 0` | |
| `cost_usd` | `DOUBLE PRECISION NOT NULL DEFAULT 0` | |
| `prompt_tokens` / `completion_tokens` / `reasoning_tokens` | `BIGINT NOT NULL DEFAULT 0` each | |
| `failed_count` | `BIGINT NOT NULL DEFAULT 0` | Rows where `outcome NOT IN ('ok', 'truncated')` |

**Indexes:** `idx_daily_spend_natural_key` (UNIQUE, `NULLS NOT DISTINCT`) on `(day, role, steerer_id, model)`
— the trigger's `ON CONFLICT` target, and the reason `NULLS NOT DISTINCT` is needed: `steerer_id`
is `NULL` for every `session_judge`/`judge_worker`/`lesson_relations`/`eval`/`lesson_proposer` call,
which never carries a steerer, and a plain `UNIQUE` treats each `NULL` as distinct.

### `llm.worker_leases`

Added in migration `20261014130000_llm_worker_leases.sql`. One row per LLM worker role, naming the
one machine that runs that role against this database. Two proxies that share a capture database
(for example the Mac and the Linux host `omit`) would otherwise each run every judge, steerer and
proposer and duplicate or collide on their work.

| Column | Type | Purpose |
|---|---|---|
| `role` | `TEXT PRIMARY KEY` | An `llm.calls.role` value: `steerer`, `session_judge`, `judge_worker`, `eval`, `lesson_proposer`, `lesson_relations`; or the name of a scheduled job that runs on one machine per database (`learning_audit`, `compressor_loop`, `usage_audit`; see `crates/mike/src/scheduled_jobs`); or `knowledge_embedding`, the project-knowledge embedding sweep (`crates/mike/src/embed_worker/knowledge.rs`). No CHECK, so a role needs no migration to be leased |
| `machine_uuid` | `UUID NOT NULL` | The machine that owns the role, from its `machine.json` |
| `heartbeat_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Database-clock time of the owner's last claim or renewal |

**Claim:** `PersistHandle::claim_llm_worker_lease` runs one `INSERT … ON CONFLICT (role) DO UPDATE …
WHERE holder = me OR heartbeat_at < now() - timeout` and reports whether a row was written. The
timeout is the larger of 180 seconds and three times the worker loop's interval. A proxy that does
not hold the lease skips the role's tick and logs it at debug level. A proxy with no machine
identity file runs every role unleased. A claim result is remembered for 30 seconds, so an
event-driven worker does not write a row per event.

**Read path:** `GET /api/capabilities` row `llm_worker_lease` lists the roles this machine holds and
the roles another machine holds, with that machine's uuid and heartbeat age.

---

## Domain: `steering`

Tracks steerer definitions, intervention attribution, and verifier fire events.

### `steering.authorities` (vocab)

Seeded: `supply_context` (1), `redirect` (2), `command` (3).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Authority code |
| `description` | `TEXT NOT NULL` | Human-readable description for UI display |

---

### `steering.modes` (vocab)

Seeded: `augment` (1), `drive` (2).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Mode code |
| `description` | `TEXT NOT NULL` | Human-readable description for UI display |

---

### `steering.verdicts` (vocab)

Seeded: `fired` (1), `suppressed_debounce` (2), `suppressed_arbitration` (3), `no_drift` (4), `gate_miss` (5), `failed_closed` (6).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | Verdict code |
| `description` | `TEXT NOT NULL` | Human-readable description for UI display |

---

### `steering.steerers`

One row per steerer definition. Steerers are intervention agents that can be enabled/disabled and deleted.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | Stable surrogate |
| `name` | `TEXT NOT NULL UNIQUE` | Internal steerer identifier |
| `display_name` | `TEXT NOT NULL` | Human-readable label, distinct from `name` |
| `description` | `TEXT NOT NULL` | Operator-facing description of what the steerer does |
| `agent_id` | `BIGINT NOT NULL` | FK → `agents.agents(id)` ON DELETE RESTRICT; agent persona used for the tier-2 agent-composition prompt fallback (`resolve_verdict_system_prompt`) |
| `authority_id` | `BIGINT NOT NULL` | FK → `steering.authorities(id)` ON DELETE RESTRICT |
| `default_mode_id` | `BIGINT NOT NULL` | FK → `steering.modes(id)` ON DELETE RESTRICT |
| `model` | `TEXT NOT NULL` | OpenRouter model id used for this steerer's judge call; empty string inherits the configured `steerer.model` fleet default (`SettingApplies::OnRestart` — a changed default takes effect on the next restart) (issue temet-ai/mike-private#125) |
| `priority` | `INTEGER NOT NULL` | Ordering/tie-break among steerers (`steerers_priority_nonneg` CHECK, `>= 0`) |
| `enabled` | `BOOLEAN NOT NULL DEFAULT FALSE` | User-facing disable toggle; does not delete the row |
| `config` | `JSONB NOT NULL DEFAULT '{}'` | Per-steerer tunables (e.g. `debounce_turns`, `confidence_threshold`, `class`, `requires`) |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `prompt` | `TEXT NULL` | Operator-authored system prompt (tier 1 of `resolve_verdict_system_prompt`); non-NULL/non-empty wins over the agent-composition lookup and the static default |
| `deleted_at` | `TIMESTAMPTZ NULL` | Terminal soft-delete marker; NULL = live, non-NULL = deleted |
| `kind` | `TEXT NOT NULL DEFAULT 'refocus'` | Steerer discriminator; `refocus` \| `memory_keeper` \| `verifier` (`steerers_kind_check` CHECK) |
| `auto_enrol` | `BOOLEAN NOT NULL DEFAULT FALSE` | `TRUE` auto-enrols every new session at session start (the generic replacement for the old `verifier` always-on special case) |
| `trigger_point` | `TEXT NOT NULL DEFAULT 'response'` | Which event type fires this steerer; `user_prompt` \| `response` (`steerers_trigger_point_check` CHECK) |
| `invocation` | `TEXT NOT NULL DEFAULT 'every_turn'` | `every_turn` judges every matched event; `pattern` judges only when `pattern` keywords match (`steerers_invocation_check` CHECK) |
| `pattern` | `TEXT NULL` | Newline-separated keyword/regex list gating evaluation when `invocation='pattern'`; NULL when `invocation='every_turn'` |
| `project_id` | `BIGINT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE; NULL = global (fires/enrols across every project); set = confines auto-enrolment to that project's sessions |

**Indexes:**
- `idx_steerers_agent_id`: `agent_id` (BTree)
- `idx_steerers_authority`: `authority_id` (BTree)
- `idx_steerers_mode`: `default_mode_id` (BTree)
- `idx_steerers_enabled`: `id WHERE enabled = TRUE` (BTree partial)
- `idx_steerers_auto_enrol_active`: `(id, default_mode_id, authority_id) WHERE auto_enrol = TRUE AND enabled = TRUE AND deleted_at IS NULL` (BTree partial) — session-start auto-enrol scan
- `idx_steerers_project_id`: `project_id WHERE NOT NULL` (BTree partial)

**Purpose of `deleted_at` vs `enabled`:** `enabled = false` is a reversible disable; `deleted_at IS NOT NULL`
is a terminal delete. A deleted steerer is dropped from the roster (list_steerers filters `deleted_at IS NULL`)
while its row and all intervention attribution history are retained — `interventions.steerer_id` is
`ON DELETE RESTRICT`, so the row must survive.

**Efficiency delivery metadata.** The existing `config` column also holds `class` and `requires`
(binary/tool availability checks). Migration `20260928070000_efficiency_steerer_class.sql` sets
`class: "efficiency"` on five named, non-deleted steerers when that key is absent. It sets `requires`
only for `ast-grep-over-grep`, recording `ast-grep`, `sg` and LSP availability checks. Existing
operator-supplied keys are preserved. The steerer registry and API retain the complete configuration.

---

### `steering.enrolments`

One row per `(session, steerer)` enrolment — which steerers are active for a session, in what mode, with what
authority. Auto-enrolled at session start for steerers with `auto_enrol = TRUE`, or explicitly via the
enrol/disenrol endpoints. `disenrolled_at IS NULL` = currently active.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `steerer_id` | `BIGINT NOT NULL` | FK → `steering.steerers(id)` ON DELETE CASCADE |
| `mode_id` | `BIGINT NOT NULL` | FK → `steering.modes(id)` ON DELETE RESTRICT |
| `authority_id` | `BIGINT NOT NULL` | FK → `steering.authorities(id)` ON DELETE RESTRICT |
| `enrolled_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `enrolled_by` | `TEXT NOT NULL` | Origin of the enrolment (e.g. `auto` for session-start auto-enrol, else an operator/session identifier) |
| `disenrolled_at` | `TIMESTAMPTZ NULL` | NULL = active enrolment |

**Constraints:**
- `uq_enrolment_session_steerer`: UNIQUE `(session_id, steerer_id)`

**Indexes:**
- `idx_enrolments_session_id`: `session_id` (BTree)
- `idx_enrolments_steerer_id`: `steerer_id` (BTree)
- `idx_enrolments_mode`: `mode_id` (BTree)
- `idx_enrolments_authority`: `authority_id` (BTree)
- `idx_enrolments_active`: `session_id WHERE disenrolled_at IS NULL` (BTree partial) — hot path: which steerers is this live session enrolled in, right now

---

### `steering.interventions`

One row per steerer evaluation (fired or not). A fired intervention links to both the captured request and
the captured response. Written best-effort from `fleet.rs` after evaluation completes.

**Two-stage delivery protocol:**
- `sent_request_id`: records which `requests.requests` row carried the intervention nudge as an injected
  `<system-reminder>`. NULL = nudge not yet sent (queued but not yet injected into a captured request, or
  fires that predate the migration).
- `delivered_request_id`: records confirmed delivery — set after the upstream returns 2xx, proving Anthropic
  accepted the request carrying the nudge. NULL = sent but not yet confirmed, or never sent.
- `processed_at`: timestamp of confirmed delivery (set alongside `delivered_request_id`).

The delivery rate is exposed via `steering.verifier_rate_v`, which counts `delivered_request_id IS NOT NULL`
(confirmed deliveries).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE` | Stable surrogate |
| `steerer_id` | `BIGINT NOT NULL` | FK → `steering.steerers(id)` ON DELETE RESTRICT |
| `enrolment_id` | `BIGINT NULL` | FK → `steering.enrolments(id)` ON DELETE CASCADE; NULL for enrolment-free fires (e.g. `verifier`) |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `verdict_id` | `BIGINT NOT NULL` | FK → `steering.verdicts(id)` ON DELETE RESTRICT; outcome of the evaluation (`fired`, `suppressed_debounce`, `no_drift`, …) |
| `mode_id` | `BIGINT NOT NULL` | FK → `steering.modes(id)` ON DELETE RESTRICT; steerer's mode at fire time |
| `authority_id` | `BIGINT NOT NULL` | FK → `steering.authorities(id)` ON DELETE RESTRICT; steerer's authority at fire time |
| `trigger_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request that triggered the evaluation |
| `gate_reason` | `TEXT NULL` | Why the config-driven gate suppressed or allowed evaluation |
| `off_thread_topic` | `TEXT NULL` | Judge's description of what the session drifted into; empty/absent when not drifting |
| `confidence` | `DOUBLE PRECISION NULL` | Judge model's confidence in the verdict, 0.0–1.0 (`interventions_confidence_range` CHECK) |
| `verdict_json` | `JSONB NULL` | Raw judge verdict payload verbatim |
| `model` | `TEXT NULL` | OpenRouter model id used for this evaluation's judge call |
| `anchor_text` | `TEXT NULL` | Text anchor the intervention quotes or targets |
| `payload_text` | `TEXT NULL` | Nudge text sent to the session; required when `verdict_id` = `fired` (`interventions_fired_has_payload` CHECK) |
| `sent_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; request carrying the nudge (attempt) |
| `sent_response_id` | `BIGINT NULL` | FK → `responses.responses(id)` ON DELETE SET NULL; response to the delivered intervention request |
| `operator_turn_id` | `BIGINT NULL` | FK → `memory.operator_turns(id)` ON DELETE SET NULL; the operator turn a fired intervention is attributed to |
| `sent_at` | `TIMESTAMPTZ NULL` | When the nudge was sent (attempt); paired with `sent_request_id` |
| `processed_at` | `TIMESTAMPTZ NULL` | Timestamp of confirmed delivery; set alongside `delivered_request_id` |
| `expires_at` | `TIMESTAMPTZ NULL` | When a pending (sent but unprocessed) intervention becomes stale; backs `idx_interventions_pending` |
| `reverted_at` | `TIMESTAMPTZ NULL` | When the intervention was reverted |
| `revert_reason` | `TEXT NULL` | Why the intervention was reverted |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `missing_evidence` | `TEXT NULL` | Judge's description of the unbacked claim behind a `verifier` fire |
| `fired_response_id` | `BIGINT NULL` | FK → `responses.responses(id)` ON DELETE SET NULL; the assistant response evaluated by the steerer (response to `trigger_request_id`), distinct from `sent_response_id` |
| `delivered_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; request confirming delivery (confirmed) |

**Indexes:**
- `idx_interventions_steerer_id`: `steerer_id` (BTree)
- `idx_interventions_enrolment_id`: `enrolment_id` (BTree)
- `idx_interventions_session_created`: `(session_id, created_at DESC)` (BTree)
- `idx_interventions_verdict`: `verdict_id` (BTree)
- `idx_interventions_mode`: `mode_id` (BTree)
- `idx_interventions_authority`: `authority_id` (BTree)
- `idx_interventions_trigger_request_id`: `trigger_request_id WHERE NOT NULL` (BTree partial)
- `idx_interventions_sent_request_id`: `sent_request_id WHERE NOT NULL` (BTree partial)
- `idx_interventions_sent_response_id`: `sent_response_id WHERE NOT NULL` (BTree partial)
- `idx_interventions_operator_turn_id`: `operator_turn_id WHERE NOT NULL` (BTree partial)
- `idx_interventions_pending`: `(session_id, expires_at) WHERE sent_at IS NOT NULL AND processed_at IS NULL` (BTree partial) — staleness sweep
- `idx_interventions_delivered_request_id`: `delivered_request_id WHERE NOT NULL` (BTree partial)
- `idx_interventions_fired_response_id`: `fired_response_id WHERE NOT NULL` (BTree partial)
- `brin_interventions_created_at`: `created_at` BRIN(32) — time-range scans

---

### `steering.thread_acceptance_criteria`

Added in migration `20260805300003_thread_acceptance_criteria.sql` ([issue #119](https://github.com/temet-ai/mike-private/issues/119)).
Bridges thread-worklog acceptance criteria into steering: one row per thread (`thread_slug` primary key),
upserted on demand by `steerer::thread_criteria::discover_for_session_text` whenever the verifier judge's
accumulated-ask text for a session plausibly mentions a registered `artifacts/threads/<slug>/worklog.md` —
no periodic sweep, no new background worker.

| Column | Type | Purpose |
|---|---|---|
| `thread_slug` | `TEXT PRIMARY KEY` | Thread identifier, from the worklog's `thread:` frontmatter field (falls back to the directory name) |
| `criteria_text` | `TEXT NULL` | The `<acceptance-criteria>` block's trimmed inner text; `NULL` when `parse_error` is set |
| `source_path` | `TEXT NOT NULL` | Worklog path the criteria were parsed from |
| `parsed_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Last (re)parse time |
| `parse_error` | `TEXT NULL` | Set when the worklog was read but no criteria block could be extracted — the row still exists so the failure is visible, never silently swallowed |

When a row's `criteria_text` is present, the verifier judge prompt uses it as the checklist source
directly instead of decomposing the accumulated ask into criteria itself (see `build_judge_user_message`).

---

### `steering.evaluations`

**Generalised (JC14, migration `20260924220000_steering_evaluations_generalise.sql`).** Every steerer evaluated past its invocation gate now writes one row per question from `run_steerer_loop`, on every path (response, `turn_end`, `turn_closed`, quiescence), resolved with its verdict or abstention; the explicit verifier-only inserts in the same-request judge and the quiescence sweep are gone. New columns: `mode` (`live` or `shadow`; a shadow would-fire is `resolved_correction` with no intervention), `bundle_id` (no FK), `question`, `trigger_point`, `trigger_request_id` (no FK; `operator_turn_id` is its mapping: the request's own turn, else the latest turn of the session created at or before it), and the label columns `outcome_label`, `label_source`, `labelled_at`, `label_evidence`. CHECKs: the four label columns are all set or all NULL; a label needs a resolved row; `next_message` and `session_end` labels only on shadow rows. Indexes `(steerer_id, question, dispatched_at DESC)` and a partial `(dispatched_at)` over unlabelled resolved shadow rows. `steering.interventions` gains `delivery_channel` (NULL, `append`, `wake`) and `addressed_to` (NULL, `session`, `parent_for_actor`) in `20260924220100_steerer_delivery.sql`.

Added in migration `20260805400001_steering_evaluations.sql` ([issue #117](https://github.com/temet-ai/mike-private/issues/117)).
`steering.interventions` records FIRES only — a "pass" verdict (the common case) is written nowhere else,
so the SPA had no signal to show while a completion-claim is being checked or once it clears clean. This
table records every `verifier` evaluation ATTEMPT, `dispatched` at the same drain-time/quiescence-sweep
call sites that invoke `steerer::fleet::judge_turn_close_now` (issue temet-ai/mike-private#120 AC2/AC5), then resolved to
`resolved_pass` or `resolved_correction` once the judge call returns. `steering.interventions`'s
fire-only semantics are unchanged — no columns added there, no rows implied.

A `dispatched` row that never resolves (the judge call timed out and its future was dropped mid-flight) is
left as-is rather than guessed at — it self-heals, since `GET /api/sessions/{id}/validation-status` always
returns the row with the latest `dispatched_at`, and the next evaluation for the session (organic retry or
the AC5 quiescence sweep) supersedes it.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `uuid` | `UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE` | Stable surrogate, returned to the caller for `resolve_evaluation` |
| `steerer_id` | `BIGINT NOT NULL` | FK → `steering.steerers(id)` ON DELETE RESTRICT; resolved internally to the seeded `verifier` steerer at insert time |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `operator_turn_id` | `BIGINT NOT NULL` | FK → `memory.operator_turns(id)` ON DELETE CASCADE; the turn being evaluated |
| `status` | `TEXT NOT NULL` | `dispatched` \| `resolved_pass` \| `resolved_correction` (CHECK constraint) |
| `intervention_uuid` | `UUID NULL` | FK → `steering.interventions(uuid)` ON DELETE SET NULL; set only for `resolved_correction` |
| `dispatched_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `resolved_at` | `TIMESTAMPTZ NULL` | |

**Indexes:**
- `idx_evaluations_steerer_id`: `steerer_id` (BTree)
- `idx_evaluations_operator_turn_id`: `operator_turn_id` (BTree)
- `idx_evaluations_session_dispatched`: `(session_id, dispatched_at DESC)` (BTree) — hot path for "latest evaluation for this session"
- `idx_evaluations_intervention_uuid`: `intervention_uuid WHERE NOT NULL` (BTree partial)

Exposed read-only via `GET /api/sessions/{id}/validation-status` (`Coverage::Exempt` — SPA-driven, not
surfaced via MCP per the demand-driven policy).

### `steering.question_policies`

Added in migration `20260925000600_turn_guard.sql` (JC16). One mode per (turn guard steerer, question): `id`, `steerer_id` (FK `steering.steerers`, cascade), `question` (`relapse`, `unbacked_wait`, `authorised_handback`, `unobserved_done`), `mode` (`off`, `shadow`, `append`, `wake`; default `shadow`), `updated_at`, `updated_via` (`migration`, `spa`, `cli`). UNIQUE `(steerer_id, question)`; CHECK `question_policies_handback_never_wakes`. JC16 writes `off` and `shadow` only, and reads `append` and `wake` as `shadow`. The same migration widens `steerers_kind_check` with `turn_guard`, widens `feature_toggles_key_check` with `turn_guard`, and seeds the `turn-guard` steerer (disabled, `"rollout": "shadow"`) and its four policy rows. The guard's `steering.evaluations` rows carry the whole `verdict_json` of the JC16 contract: versions, trigger, exemption, facts, gates per question (with arm and the other arm), the model output per question, `first_vote`, `quorum`, `reach`, `actor`, `relapse_anchor_turn_id`, `would_be_superseded_by`, `recall_sample`, `semantic` (reserved, NULL) and `input_chars`.

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `steerer_id` | bigint NOT NULL REFERENCES steering.steerers(id) ON DELETE CASCADE |
| `question` | text NOT NULL CHECK (question IN ('relapse', 'unbacked_wait', 'authorised_handback', 'unobserved_done')) |
| `mode` | text NOT NULL DEFAULT 'shadow' CHECK (mode IN ('off', 'shadow', 'append', 'wake')) |
| `updated_at` | timestamptz NOT NULL DEFAULT now() |
| `updated_via` | text NOT NULL DEFAULT 'migration' CHECK (updated_via IN ('migration', 'spa', 'cli')) |

### `steering.question_promotions`

Added in migration `20260925000700_steer_question_promotion.sql` (JC17). Append-only history of every mode change of a guard question to or from `append` or `wake`: `policy_id` (FK `steering.question_policies`), `from_mode`, `to_mode`, `action` (`promote`, `demote`, `auto_demote`), `decided_via` (`spa`, `api`, `cli`, `auto`), `decided_at`, `gate_passed`, `gate_json` (the whole gate evidence), `override_reason` (required for a promotion against a failing gate), `replaces_steerer_id` / `restored_steerer_id` (the verifier on the unobserved_done replacement and its revert), `change_log_id` (no FK). The same migration adds `holdback_pct` and `holdback_salt` to `steering.question_policies` (the arm draw is `sha256(salt:steerer_id:question:anchor_turn_id)`), widens its `updated_via` with `api` and `auto`; adds `arm` (`treated`, `holdback`; live rows only), `holdback_draw`, `arm_anchor_turn_id`, `due_at` and `reach_at_due` (`reachable`, `not_idle`, `no_stream`) to `steering.evaluations`, and lets automatic labels land on held-back rows; and adds `deliver_after`, `wake_state`, `wake_claimed_at`, `wake_resolved_at` and `wake_attempts` to `steering.interventions` (the deferred push of a wake).

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `policy_id` | bigint NOT NULL REFERENCES steering.question_policies(id) ON DELETE CASCADE |
| `from_mode` | text NOT NULL CHECK (from_mode IN ('off', 'shadow', 'append', 'wake')) |
| `to_mode` | text NOT NULL CHECK (to_mode IN ('off', 'shadow', 'append', 'wake')) |
| `action` | text NOT NULL CHECK (action IN ('promote', 'demote', 'auto_demote')) |
| `decided_via` | text NOT NULL CHECK (decided_via IN ('spa', 'api', 'cli', 'auto')) |
| `decided_at` | timestamptz NOT NULL DEFAULT now() |
| `gate_passed` | boolean NOT NULL |
| `gate_json` | jsonb NOT NULL |
| `override_reason` | text |
| `replaces_steerer_id` | bigint REFERENCES steering.steerers(id) ON DELETE SET NULL |
| `restored_steerer_id` | bigint REFERENCES steering.steerers(id) ON DELETE SET NULL |
| `change_log_id` | bigint |

### `steering.guard_thresholds`

Added in migration `20260925000800_guard_thresholds.sql` (JC18). Append-only history of calibrated guard thresholds, one row per (steerer, question, scope, project) per `eval.rollups` generation: the label basis, the question and threshold modes, the labelled count, the bar, floor, ceiling and static threshold, the candidate, the threshold in force after the row, the `decision` (`applied`, `shadow_only`, `held_hysteresis`, `held_cooldown`, `held_wake_floor`, `insufficient`, `mode_off`) and every bin used in `detail`. The threshold in force is the latest `applied` row per key. `20260925000801_change_log_calibration.sql` widens `config.change_log` with `guard_threshold` and `directive_decision` subjects and the `calibration` via, and lets `steer_calibration` rates carry `metric = 'precision'` (JC18's candidate bins).

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `steerer_id` | bigint NOT NULL |
| `question` | text NOT NULL CHECK (question IN ('relapse', 'unbacked_wait', 'authorised_handback', 'unobserved_done', 'fire')) |
| `scope` | text NOT NULL CHECK (scope IN ('project', 'global')) |
| `project_id` | bigint |
| `computed_at` | timestamptz NOT NULL DEFAULT now() |
| `rollup_id` | bigint NOT NULL |
| `rollup_finished_at` | timestamptz NOT NULL |
| `window_start` | timestamptz NOT NULL |
| `window_end` | timestamptz NOT NULL |
| `label_basis` | text NOT NULL CHECK (label_basis IN ('auto_shadow_gold_relayed', 'holdback_gold_relayed', 'weighted_by_source')) |
| `question_mode` | text NOT NULL CHECK (question_mode IN ('off', 'shadow', 'append', 'wake')) |
| `threshold_mode` | text NOT NULL CHECK (threshold_mode IN ('off', 'shadow', 'apply')) |
| `n_labelled` | integer NOT NULL CHECK (n_labelled >= 0) |
| `precision_bar` | double precision NOT NULL CHECK (precision_bar > 0 AND precision_bar <= 1) |
| `threshold_floor` | real NOT NULL |
| `threshold_ceiling` | real NOT NULL |
| `static_threshold` | real NOT NULL |
| `candidate_threshold` | real |
| `applied_threshold` | real NOT NULL |
| `decision` | text NOT NULL CHECK (decision IN ('applied', 'shadow_only', 'held_hysteresis', 'held_cooldown', 'held_wake_floor', 'insufficient', 'below_bar', 'mode_off')) |
| `detail` | jsonb NOT NULL DEFAULT '{}'::jsonb |

Migration `20261014150000_threshold_pass_runs.sql` widens the table in three ways. `question` also accepts `fire`, a threshold on the confidence of every fire of the verifier or the monitor-over-sleep steerer (scope `global`, `steerer_id` names the steerer, and `detail.steerer` its name). `label_basis` also accepts `weighted_by_source`: the operator, session-reply, session-end and next-message labels, each counted at the weight set in `guard.threshold.weight.*`, with `detail.by_source` holding the raw `k`, `n` and the weight per source. `decision` also accepts `below_bar`: enough weighted labels, but no confidence bin has a Wilson lower bound at or above the precision bar, so the threshold in force is kept; `insufficient` now means fewer weighted labels than `guard.threshold.min_labelled`.

### `steering.threshold_pass_runs`

Added in migration `20261014150000_threshold_pass_runs.sql`. One row per `eval.rollups` generation the threshold pass handled, written whether or not the pass wrote threshold rows, so a silent pass is visible. `outcome` is `wrote` (rows written with labels counted), `no_labels` (rows written, no question with a label), `mode_off` (`guard.threshold.mode` is off, rows written with decision `mode_off`), `no_calibration_rows` (the rollup carries no `steer_calibration` rows) or `error` (the pass failed; `attempts` counts the tries and `detail.error` holds the last message). `n_by_source` holds the labelled candidates counted per question, or per `fire:<steerer>`, and per label source before weighting. `GET /api/cross-session/thresholds` returns the latest runs as `passes`.

| Column | Definition |
|---|---|
| `rollup_id` | bigint PRIMARY KEY |
| `first_ran_at` | timestamptz NOT NULL DEFAULT now() |
| `last_ran_at` | timestamptz NOT NULL DEFAULT now() |
| `attempts` | integer NOT NULL DEFAULT 1 CHECK (attempts >= 1) |
| `outcome` | text NOT NULL CHECK (outcome IN ('wrote', 'no_labels', 'mode_off', 'no_calibration_rows', 'error')) |
| `threshold_mode` | text NOT NULL CHECK (threshold_mode IN ('off', 'shadow', 'apply')) |
| `rows_written` | integer NOT NULL CHECK (rows_written >= 0) |
| `applied` | integer NOT NULL CHECK (applied >= 0) |
| `n_by_source` | jsonb NOT NULL DEFAULT '{}'::jsonb |
| `detail` | jsonb NOT NULL DEFAULT '{}'::jsonb |

### `memory.directive_proposals` and `memory.directive_proposal_evidence`

Added in migration `20260925000802_directive_proposals.sql` (JC18). A proposal is a candidate standing directive found by SQL: a source (`repeated_directive`, `confirmed_handback`), a scope (`project`, `no_project`, `operator`), a class and polarity, a deterministic `proposed_text` from the operator's own words (whole), counts and window, `rollup_id`, and a lifecycle (`open`, `approved`, `rejected`, `superseded`, `expired`). One open proposal per key (partial unique index `uq_directive_proposals_open_key`). Evidence rows reference the dossier entry or evaluation behind each item; the operator's message is read in full at display time. The migration adds the `directive_proposer` toggle (off).

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `uuid` | uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE |
| `source` | text NOT NULL CHECK (source IN ('repeated_directive', 'confirmed_handback', 'recurring_failure_tag', 'recurring_pattern')) |
| `scope` | text NOT NULL CHECK (scope IN ('project', 'no_project', 'operator')) |
| `project_id` | bigint REFERENCES projects.projects(id) ON DELETE SET NULL |
| `class` | text NOT NULL CHECK (length(class) BETWEEN 1 AND 200) |
| `polarity` | text NOT NULL DEFAULT 'none' CHECK (polarity IN ('allow', 'forbid', 'none')) |
| `status` | text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'approved', 'rejected', 'superseded', 'expired')) |
| `proposed_kind` | text NOT NULL CHECK (proposed_kind IN ('correction', 'convention', 'warning', 'preference')) |
| `proposed_text` | text NOT NULL |
| `evidence_count` | integer NOT NULL CHECK (evidence_count > 0) |
| `session_count` | integer NOT NULL CHECK (session_count > 0) |
| `first_evidence_at` | timestamptz NOT NULL |
| `last_evidence_at` | timestamptz NOT NULL |
| `window_days` | integer NOT NULL CHECK (window_days > 0) |
| `rollup_id` | bigint NOT NULL |
| `proposer_version` | text NOT NULL |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `decided_at` | timestamptz |
| `decision_note` | text |
| `approved_text` | text |
| `superseded_by` | bigint REFERENCES memory.directive_proposals(id) ON DELETE SET NULL |

### `steering.verifier_rate_v`

Materialized view computing verifier fire rate per project. Closed turns read the materialized cache columns on
`memory.operator_turns`. The single open turn computes live via two `GROUP BY` subqueries.

Columns: `project_path`, `fires_count`, `corrected_count`, `delivered_count`, `corrected_rate`, `delivered_rate`.

`corrected_rate` and `delivered_rate` are NULL when `fires_count = 0` (no fires for that project).

`delivered_count` counts interventions where `delivered_request_id IS NOT NULL` (confirmed deliveries).

---

### `steering.eir_timing_v`

Added in migration `20260803000001_eir_timing_v.sql` ([issue #118](https://github.com/temet-ai/mike-private/issues/118)).
Companion to `verifier_rate_v` — that view answers "how often does a fire correlate with a correction";
this one answers "how early or late, in operator-turn terms". One row per verifier fire and one row per
corrected operator turn, discriminated by `row_kind` (`fire`/`correction`).

Turn ordinal is `row_number()` over a session's operator turns ordered by `start_request_id` (immutable
key). Nearest correction (for a fire row) or nearest preceding fire (for a correction row) is picked via a
join + per-partition `row_number()`, never a correlated scalar subquery.

Columns: `row_kind`, `intervention_id`, `session_id`, `trigger_request_id`, `event_at`, `fire_turn_id`,
`fire_turn_ordinal`, `correction_turn_id`, `correction_turn_ordinal`, `corrected_at`, `signed_gap`,
`timing_class`, `session_turn_count`, `session_length_segment`.

`timing_class` is `early`/`on-time`/`late`/`never` for `fire` rows (signed gap = correction ordinal minus
fire ordinal; positive means the fire preceded the correction) and `fire_preceded`/`no_fire_preceded` for
`correction` rows. `session_length_segment` splits `short` (<8 operator turns) vs `long` (>=8) sessions.

Query with `select * from steering.eir_timing_v where row_kind = 'fire'`.

---

## Domain: `mailbox`

Added in migration `20260630000002_session_mailbox.sql`. Inter-session message delivery and reminder injection tracking.

### `mailbox.messages`

One row per inter-session message. States, in order: queued (`claimed_at IS NULL`) → claimed by a turn
for injection (`claimed_at IS NOT NULL`) → sent (`sent_request_id IS NOT NULL`) → **consumed**
(`consumed_at IS NOT NULL`, explicit ack) or **dead-lettered** (`dead_lettered_at IS NOT NULL`, redelivery
cap reached without an ack). `consumed_at`/`dead_lettered_at`/`delivery_attempts` were added in migration
`20260710000001_mailbox_redelivery.sql` ([issue #169](https://github.com/temet-ai/mike-private/issues/169)):
a sent-but-unconsumed row is redelivered on later eligible primary turns rather than delivered exactly once,
closing the gap where injected-on-the-wire content never enters the recipient's own conversation history and
is gone from its context on the next turn.

The `from_session_id` is a convenience reply pointer — not an identity or auth claim. It is NULL for anonymous
sends and SET NULL if the sending session is deleted.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `message_uuid` | `UUID NOT NULL UNIQUE` | Stable surrogate |
| `to_session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `from_session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; optional reply pointer |
| `body` | `TEXT NOT NULL` | Message text |
| `queued_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When message was queued |
| `claimed_at` | `TIMESTAMPTZ NULL` | Two-phase sentinel: set when a turn claims this row for injection, before the in-memory reminder is consumed. Prevents re-delivery on restart. |
| `sent_request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; request that first carried this message as an injected reminder |
| `sent_at` | `TIMESTAMPTZ NULL` | Timestamp of the first delivery (set alongside `sent_request_id`) |
| `consumed_at` | `TIMESTAMPTZ NULL` | Stamped by `POST /api/sessions/{id}/messages/{uuid}/ack` — explicit recipient acknowledgment; stops redelivery |
| `dead_lettered_at` | `TIMESTAMPTZ NULL` | Stamped when `delivery_attempts` reaches `mailbox_redelivery_max_attempts` without an ack — stops redelivery |
| `delivery_attempts` | `INT NOT NULL DEFAULT 0` | Count of successful envelope injections (first send + every redelivery) |

**Indexes:**
- `idx_mailbox_messages_from_session`: `from_session_id WHERE NOT NULL` (BTree partial) — reply pointer queries
- `idx_mailbox_messages_sent_request`: `sent_request_id WHERE NOT NULL` (BTree partial) — delivery confirmation
- `idx_mailbox_messages_pending`: `(to_session_id, id) WHERE sent_request_id IS NULL` (BTree partial) — reload/drain lookup: never-sent rows for one recipient, oldest first
- `idx_mailbox_messages_sender_unnotified`: `(queued_at) WHERE sender_notified_at IS NULL AND from_session_id IS NOT NULL AND to_session_id IS NOT NULL AND sent_at IS NULL` (BTree partial) — the mailbox reaper tells a sender once when its message was not delivered, and `sender_notified_at` records that it did
- `idx_mailbox_messages_redelivery`: `(to_session_id, id) WHERE consumed_at IS NULL AND dead_lettered_at IS NULL AND sent_at IS NOT NULL` (BTree partial) — reload/drain lookup: already-sent, still-eligible rows for one recipient, oldest first

**Staleness/dead-letter visibility** ([issue #166](https://github.com/temet-ai/mike-private/issues/166)):
`GET /api/sessions/{id}/messages` computes `pending_minutes` and `stale` at read time for any row
with `status == "pending"` — no schema change, just a projection against `queued_at` and the
`mailbox_stale_minutes` setting (default `10`, env `MIKE_MAILBOX_STALE_MINUTES`, `/api/settings`
key `mailbox_stale_minutes`). Both fields are `null` for `claimed`/`delivered`/`consumed`/`dead_lettered` rows.
`status` is derived most-terminal-first: `consumed` > `dead_lettered` > `delivered` > `claimed` > `pending`.

**Redelivery** ([issue #169](https://github.com/temet-ai/mike-private/issues/169)): governed by the
session-scoped `mailbox_redelivery` feature toggle (default ON — OFF reverts to first-delivery-only, never
zero delivery) and the `mailbox_redelivery_max_attempts` boot setting (default `5`, env
`MIKE_MAILBOX_REDELIVERY_MAX_ATTEMPTS`; counts the first send as attempt 1). The envelope
(`envelope_mailbox_message`) carries the exact `curl -X POST .../messages/{uuid}/ack` command so the
recipient can stop redelivery directly rather than relying on the pre-#169 restate-it-yourself fallback,
which remains as a fallback for a recipient that never runs the curl.

**Ack contract** — `POST /api/sessions/{id}/messages/{uuid}/ack`: `404` when `{id}`/`{uuid}` do not name a
row addressed to that session; `503` when persistence is disabled; **`200` idempotent** (not `409`) when the
row was already consumed — returns the ORIGINAL `consumed_at`, since a retried ack from a flaky network is
indistinguishable from a first ack and should not fail. `200` `{ message_uuid, consumed_at }` on success.
Acking also drops any in-memory redelivery entry for that message immediately (`RequestStore::remove_mailbox_reminder`)
rather than waiting for the next drain's `bump_redelivery_attempt` call to fail closed.

### `mailbox.message_context_items`

Added by `20260810100004_mailbox_message_context.sql`. Structured context items attached to a
mailbox message — a request id, a commit sha, a URL — deep-linking pointers the recipient can jump
to, distinct from the free-text body. `kind` is a closed set, revalidated at the REST layer so an
unknown kind 400s naming it. Written with the message by
`store/persistence/mailbox/thread_send.rs` (`insert_message_with_context`, the `context` field of
a send); read back by `session_reads.rs` and `thread_reads.rs` into the message DTOs.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `message_id` | `BIGINT NOT NULL` | FK → `mailbox.messages(id)` ON DELETE CASCADE |
| `kind` | `TEXT NOT NULL` | CHECK `IN ('request', 'session', 'thread', 'message', 'commit', 'file', 'issue', 'url', 'note')` |
| `value` | `TEXT NOT NULL` | The pointer (id, sha, path, URL, ...) |
| `label` | `TEXT NULL` | Optional display label |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_mailbox_message_context_items_message`: `message_id` (BTree)

---


### `mailbox.failed_sends`

Added in migration `20260709000001_mailbox_failed_sends.sql` ([issue #166](https://github.com/temet-ai/mike-private/issues/166)).
Durable, queryable evidence for an unsuccessful `POST /api/sessions/{to}/message` — previously a
wrong-recipient guess or malformed request left zero trace anywhere.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `attempted_by_session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; resolved from the caller's `from` UUID when supplied and known, else NULL — never fails the write |
| `attempted_to` | `TEXT NOT NULL` | The raw recipient string the caller passed, stored verbatim even when it resolves to no session |
| `reason` | `TEXT NOT NULL` | `CHECK`-constrained to the `mailbox_error()` taxonomy: `empty_message`, `invalid_from_session_id`, `session_not_found`, `from_session_not_found`, `database_error` |
| `attempted_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When the failed attempt happened |
| `request_id` | `BIGINT NULL` | FK → `requests.requests(id)` ON DELETE SET NULL; the request the send happened during, when known |

**Indexes:** partial BTree on both FKs (sparse columns, matching `mailbox.messages`' idiom); BRIN
on `attempted_at` (append-mostly time series).

**Verifier/unread-mail gate** ([issue #166](https://github.com/temet-ai/mike-private/issues/166)):
the `verifier` steerer (`steering.steerers`, kind `verifier`) forces a fire — overriding the
judge's "no intervention needed" verdict — when `PersistHandle::has_pending_inbound_mail` finds an
unclaimed inbound row for the session under evaluation. Wired in `run_steerer_loop`
(`crates/mike/src/steerer/fleet.rs`), backed by a single `EXISTS` semi-join on
`idx_mailbox_messages_pending` — a session cannot be judged legitimately "done" while a
coordinator's message sits unread.

---

### `mailbox.threads`

Added by `20260810100001_mailbox_threads.sql`, which also gave `mailbox.messages` its `thread_id`
(FK → `mailbox.threads(id)` ON DELETE SET NULL) and `in_reply_to_id` columns. A conversation thread
between sessions: the unit an operator lists, filters, tags and resolves. Surfaced by
`GET`/`POST /api/threads`, `GET /api/threads/{thread_uuid}` (`read_thread`),
`POST /api/threads/{thread_uuid}/message`, `POST /api/threads/{thread_uuid}/resolve`
(`resolve_thread`) and `GET /api/sessions/{id}/threads` (`get_session_threads`); persistence in
`store/persistence/mailbox/thread_core.rs`, `thread_send.rs`, `thread_reads.rs` and
`thread_summaries_tail.sql`. Project knowledge search (`project_knowledge/search.sql`) ranks thread
messages too.

`project_id` is placed by trigger `messages_place_thread_in_project` (AFTER INSERT OR UPDATE OF
`thread_id` on `mailbox.messages`, `20261006000000_comms_thread_project_and_tags.sql`) from the
participants' real project (`mailbox.thread_project_id()`), so no thread sits under the
"(no project)" placeholder. `fleet_uuid` (`20261005020000_fleet_comms_thread.sql`) marks a fleet's
comms thread — one per fleet, `thread_uuid = fleet_uuid` — into which `mailbox.file_fleet_mail()`
files mail to or from the fleet's seats that has no thread of its own. Row changes NOTIFY through
`trg_mailbox_threads_row_changed` / `trg_mailbox_threads_row_changed_update`
(`20260810100002_mailbox_row_change_triggers.sql`, redefined by
`20261102240000_row_changed_notify_only_on_relevant_change.sql` to fire on UPDATE only when a
column changed).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `thread_uuid` | `UUID NOT NULL UNIQUE` | Stable surrogate; the id in routes and tool answers |
| `subject` | `TEXT NOT NULL` | |
| `project_id` | `BIGINT NULL` | FK → `projects.projects(id)` ON DELETE SET NULL; placed by trigger, see above |
| `created_by_session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; counted as a participant |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_message_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Bumped on every message appended (new send, reply, or retroactive adoption); the sort key of the recent-threads views |
| `resolved_at` | `TIMESTAMPTZ NULL` | Set once by `POST /api/threads/{thread_uuid}/resolve` |
| `fleet_uuid` | `UUID NULL` | Set on a fleet's comms thread; unique where set |

**Indexes:**
- `idx_mailbox_threads_project_recent`: `(project_id, last_message_at DESC)` (BTree) — `GET /api/threads?project=`
- `idx_mailbox_threads_created_by`: `created_by_session_id WHERE created_by_session_id IS NOT NULL` (BTree partial)
- `threads_fleet_uuid`: `fleet_uuid WHERE fleet_uuid IS NOT NULL` (BTree unique partial)

---

### `mailbox.tags`

Added by `20260810100003_mailbox_tags.sql`. Open vocabulary of thread tags, interned on demand: an
unknown code on a send is inserted here, not rejected (`attach_tags_to_thread`,
`store/persistence/mailbox/tags.rs`). Tags attach to threads, not messages. Since
`20261006000000_comms_thread_project_and_tags.sql` a code is stored normalized by
`mailbox.normalize_tag()` — lower case, spaces as hyphens, a leading `#` dropped, a bare number as
`issue-<n>`, and nothing without a letter — enforced by CHECK; that migration rewrote the existing
rows and moved their `thread_tags`. `GET /api/tags` lists every tag with its usage count.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `code` | `TEXT NOT NULL UNIQUE` | The normalized tag |
| `hue` | `SMALLINT NULL` | Read into the tag DTO; nothing writes it today |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- `tags_code_normalized`: `code = mailbox.normalize_tag(code)`

Seeded (ids assigned by the identity sequence): `handoff`, `blocked`, `review`, `merge`,
`redeploy`, `heads-up`, `question`, `decision`.

---

### `mailbox.thread_tags`

Added by `20260810100003_mailbox_tags.sql`. The many-to-many between threads and tags. Written by
`attach_tags_to_thread` (`store/persistence/mailbox/tags.rs`); read by the thread listings
(`thread_reads.rs`, `thread_summaries_tail.sql`) and the `GET /api/tags` usage count.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT NOT NULL` | FK → `mailbox.threads(id)` ON DELETE CASCADE |
| `tag_id` | `BIGINT NOT NULL` | FK → `mailbox.tags(id)` ON DELETE CASCADE |

**Constraints:**
- PRIMARY KEY `(thread_id, tag_id)`

**Indexes:**
- `idx_mailbox_thread_tags_tag`: `tag_id` (BTree)

---




## Domain: `remote_control`

Added in migration `20260903000001_remote_control_channel.sql` ([issue #230](https://github.com/temet-ai/mike-private/issues/230)) —
capture for the Remote Control channel every Claude Code session opens to `api.anthropic.com` under
`/v1/code/sessions/<cse_id>/...`. These requests carry no `x-claude-code-session-id` header, so they
are keyed on the `cse_` id parsed from the URL path instead. Each table has its own section at the
end of this domain; the prose here covers what `20260908000003_remote_control_session_link.sql`
([issues temet-ai/mike-private#231/#232](https://github.com/temet-ai/mike-private/issues/231)) added on top of it.

**`remote_control.channels`** gained five columns and a channel-to-session link. `local_session_uuid`,
`local_cwd` and `client_version` are read from the first kept `attachment` or `system` payload item
that carries `payload.sessionId` (every sampled `attachment` payload did) — see
`proxy::remote_control::route::payload::payload_session_link`. They are first-write-wins as a set: a
session does not change its own identity, working directory or client version mid-run, so once
`local_session_uuid` is set, none of the three move again. `session_id` (present since migration
20260903000001 but never written until now) resolves lazily from `local_session_uuid` against
`sessions.sessions.session_uuid` and then stays sticky — the lookup only runs while it is still NULL.
`post_turn_summary` and `post_turn_summary_at` are `PUT /worker`'s other shape: instead of the
`idle`/`running`/`requires_action` `worker_status` enum, some calls carry
`external_metadata.post_turn_summary`, a free-text statement of what the session just did, kept as
JSONB exactly as sent. Latest-write-wins, like `worker_status` — a running statement, not an identity
fact.

**`remote_control.worker_status_transitions`** is new: one row per observed change in a channel's
`worker_status`, appended only when the incoming status differs from what was stored — never on every
heartbeat. `worker_status`/`worker_status_at` on the channel row stay the current reading; this table
is the history they cannot hold on their own. `from_status` is `NULL` for a channel's first-ever
observed status. No `CHECK` on `from_status`/`to_status`: the vocabulary is open, matching
`channels.worker_status` itself. Read via `PersistHandle::fetch_worker_status_history(session_uuid,
limit)`, newest first, across every channel that has ever linked to the session (a reconnect mints a
new `cse_` id and therefore a new channel row). Its production caller today is a best-effort flapping
check (`proxy::remote_control::capture::persist::warn_on_flapping_status`), run after a call that
carried a `worker_status`: three or more transitions inside sixty seconds log a warning. No REST or
MCP route consumes the history directly yet.

**Read surface.** `PersistHandle::fetch_channel_snapshots(session_uuids)` is the batch read the
workspace roster merge (`store::persistence::workspace::roster`, documented in the `workspace` domain
below) uses — cse_id, the link, both heartbeat/idle/interval readings, `worker_status` +
`worker_status_at`, a computed `stream_open` boolean (`stream_opened_at IS NOT NULL AND
(stream_closed_at IS NULL OR stream_closed_at < stream_opened_at)`) alongside the raw
`stream_opened_at`, `last_delivery_at`, and the post-turn-summary pair. It reads `channels` only,
never `calls` — `calls` grows by roughly 180,000 rows a day and neither the roster nor any liveness
query may scan it.

**`remote_control.injections`** (migration
`20260908000004_remote_control_injections.sql`) stores the exact bytes of each
mike-authored mailbox frame, its event UUID, channel and mailbox message, and
the reservation, emission, failure and acknowledgement timestamps. These
frames have no SSE `id:` line, so they leave the client's upstream sequence
cursor untouched. A database reservation and mailbox claim precede emission;
a failed or timed-out emission releases the claim for ordinary turn delivery.
Successful emission delays retry for thirty seconds while the client replies.

Only a captured `POST /worker/events/delivery` update with
`status: "received"`, the same event UUID and the same channel stamps
`acked_at`/`ack_call_id` and the mailbox's `sent_at`/`consumed_at`. Duplicate
acknowledgements preserve the first timestamps and do not increment attempts.
`sent_request_id` stays NULL: an SSE delivery has no inference request ID.
An acknowledgement proves client receipt; it does not prove that the model
acted on the message. The delivery request and upstream response continue
unchanged, including any upstream rejection of a locally minted event ID.

Stream ownership is process-local, with cleanup scoped to the exact connection.
Closing an older connection also leaves a newer connection's database
`stream_closed_at` unchanged. Upstream bytes remain in their original order;
the relay inserts frames only between complete SSE events, with bounded wake
deadlines even while the client applies backpressure.

---

### `remote_control.channels`

Added by `20260903000001_remote_control_channel.sql`; the link and summary columns by
`20260908000003_remote_control_session_link.sql`. One row per Remote Control `cse_` channel, with
the liveness facts rolled up so the liveness query and the workspace roster never scan
`remote_control.calls`. The capture path (`proxy::remote_control::capture::persist`, through
`store/persistence/remote_control/touch_channel.sql` and `record_calls.sql`) upserts the row on
every call, so it takes ~180,000 updates a day; the table is created `WITH (fillfactor = 70,
autovacuum_vacuum_scale_factor = 0.05, autovacuum_vacuum_threshold = 25)` so those updates stay
heap-only, and none of the hot columns is indexed. `session_id` is resolved from
`local_session_uuid` (`link_channel_from_request_body.sql`, the sweep in `link_sweep.rs`) and then
stays sticky. Read by `fetch_channel_snapshots` (the roster merge), `fetch_worker_status_history`,
the launch channel checks (`launch/channel_exists.sql`, `session_has_channel.sql`), the steering
promotion reach query, `renew_lease_on_heartbeat.sql` and the session-flow cursor.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `cse_id` | `TEXT NOT NULL UNIQUE` | The `cse_` id from `/v1/code/sessions/<cse_id>/...`; the natural key |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; resolved lazily from `local_session_uuid`, sticky once set |
| `client_session_id` | `TEXT NULL` | The `session_id` field inside the payloads (repeats `cse_id` in the 2.1.259 client); stored so a divergence is visible |
| `worker_epoch` | `BIGINT NULL` | The worker generation the client addresses; from `POST /bridge`, echoed on worker calls |
| `first_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Touched by every call on the channel |
| `last_heartbeat_at` | `TIMESTAMPTZ NULL` | Last `POST /worker/heartbeat` |
| `last_idle_seconds` | `INTEGER NULL` | `idle_seconds` from the last heartbeat body (client `idleTracker`) |
| `last_heartbeat_interval_seconds` | `INTEGER NULL` | `current_interval_seconds` from the same heartbeat |
| `worker_status` | `TEXT NULL` | Latest `PUT /worker` status; open set, no CHECK |
| `worker_status_at` | `TIMESTAMPTZ NULL` | |
| `last_delivery_at` | `TIMESTAMPTZ NULL` | Last `POST /worker/events/delivery` |
| `stream_opened_at` | `TIMESTAMPTZ NULL` | `GET /worker/events/stream` opened |
| `stream_closed_at` | `TIMESTAMPTZ NULL` | That stream closed |
| `local_session_uuid` | `UUID NULL` | The local session, from the first kept `attachment`/`system` payload carrying `payload.sessionId`; first-write-wins |
| `local_cwd` | `TEXT NULL` | `payload.cwd` from the same payload; first-write-wins |
| `client_version` | `TEXT NULL` | `payload.version` (the Claude Code version) from the same payload; first-write-wins |
| `post_turn_summary` | `JSONB NULL` | `external_metadata.post_turn_summary` from the latest `PUT /worker` that carried one; latest-wins |
| `post_turn_summary_at` | `TIMESTAMPTZ NULL` | |

**Constraints:**
- `channels_last_idle_seconds_nonneg`: `last_idle_seconds IS NULL OR last_idle_seconds >= 0`
- `channels_heartbeat_interval_positive`: `last_heartbeat_interval_seconds IS NULL OR last_heartbeat_interval_seconds > 0`

**Indexes:**
- `idx_rc_channels_session_id`: `session_id WHERE session_id IS NOT NULL` (BTree partial)
- `brin_rc_channels_first_seen_at`: `first_seen_at` (BRIN, `pages_per_range = 32`)
- `idx_rc_channels_local_session_uuid`: `local_session_uuid WHERE local_session_uuid IS NOT NULL` (BTree partial)

---

### `remote_control.calls`

Added by `20260903000001_remote_control_channel.sql`. One row per HTTP call on a channel, recorded
once after the response completes by `record_calls.sql` (bound from
`proxy::remote_control::capture` in channel batches); the one exception is
`/worker/events/stream`, whose row opens with `completed_at` NULL and is closed later. Which paths
store full bodies is decided by `KNOWN_ENDPOINTS` / the body-capture policy in
`proxy/remote_control/route.rs`, not here: `request_body IS NULL AND request_body_size IS NOT NULL`
means a body existed and its bytes were not stored; `request_body_size IS NULL` means there was no
body. A batch call's own `request_body` is NULL while its kept items live in
`remote_control.call_payloads`; the size and digest still describe the whole batch. The CHECKs
refuse any stored body whose length disagrees with its recorded size, so a slice cannot be written.
Grows by ~180,000 rows a day; no roster or liveness query may scan it.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `channel_id` | `BIGINT NOT NULL` | FK → `remote_control.channels(id)` ON DELETE CASCADE |
| `method_id` | `BIGINT NOT NULL` | FK → `requests.methods(id)`; deliberately unindexed |
| `path` | `TEXT NOT NULL` | The suffix after `/v1/code/sessions/<cse_id>` (`/worker/heartbeat`, `/worker/events`, `/bridge`, ...); text, not a vocabulary id, to avoid row-lock contention on the write path |
| `http_status` | `INTEGER NULL` | |
| `request_body` | `BYTEA NULL` | Full request bytes, or NULL when the policy elided them |
| `request_body_sha256` | `BYTEA NULL` | Raw 32-byte digest; a fingerprint, never a key |
| `request_body_size` | `INTEGER NULL` | NULL = no body; 0 = an empty one |
| `response_body` | `BYTEA NULL` | |
| `response_body_sha256` | `BYTEA NULL` | |
| `response_body_size` | `INTEGER NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `completed_at` | `TIMESTAMPTZ NULL` | NULL while a call is open (only the SSE stream) |
| `payload_count` | `INTEGER NULL` | Items the batch held; NULL for a call with no batch |
| `payload_kept_count` | `INTEGER NULL` | Items that became `call_payloads` rows |
| `payload_type_counts` | `JSONB NULL` | Item counts by payload type, including the dropped types |
| `is_compaction` | `BOOLEAN NULL` | From `POST /worker/internal-events`; NULL on every other path |
| `session_agent_id` | `TEXT NULL` | From the same envelope: the sub-agent that produced the event |
| `preserved_event_ids` | `TEXT[] NULL` | From the same envelope: the events that survived a context compaction |

**Constraints:**
- `calls_request_sha_is_sha256`: `request_body_sha256 IS NULL OR octet_length(request_body_sha256) = 32`
- `calls_response_sha_is_sha256`: `response_body_sha256 IS NULL OR octet_length(response_body_sha256) = 32`
- `calls_request_body_size_nonneg`: `request_body_size IS NULL OR request_body_size >= 0`
- `calls_response_body_size_nonneg`: `response_body_size IS NULL OR response_body_size >= 0`
- `calls_request_body_is_whole`: `request_body IS NULL OR octet_length(request_body) = request_body_size`
- `calls_response_body_is_whole`: `response_body IS NULL OR octet_length(response_body) = response_body_size`
- `calls_payload_counts_nonneg`: both counts NULL or `>= 0`
- `calls_payload_kept_within_count`: `(payload_count IS NULL) = (payload_kept_count IS NULL)` and `payload_kept_count <= payload_count`

**Indexes:**
- `idx_rc_calls_channel_created_at`: `(channel_id, created_at DESC)` (BTree) — the per-channel call history, and the channel FK's cascade
- `brin_rc_calls_created_at`: `created_at` (BRIN, `pages_per_range = 32`)

---

### `remote_control.call_payloads`

Added by `20260903000001_remote_control_channel.sql`. One row per payload item of a batch Remote
Control call (`POST /worker/events`, up to 100 `{payload, ephemeral?}` items) that the capture policy
KEPT. Items whose bytes duplicate what `/v1/messages` already captures (`stream_event`, `user`,
`assistant`, `message`) get no row — they are only counted in the parent call's
`payload_type_counts` — so a `permission_request`, `task_status`, `session_state_changed` or any
unrecognised type is stored once, in full. Written by `record_calls.sql`; the proxy reads the
`attachment`/`system` items for the session link before storage, and no production query reads this
table today (integration tests only).

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `call_id` | `BIGINT NOT NULL` | FK → `remote_control.calls(id)` ON DELETE CASCADE |
| `ordinal` | `INTEGER NOT NULL` | Zero-based position within the batch, preserved although dropped items get no row |
| `payload_type` | `TEXT NOT NULL` | The client's own payload `type`; open set, no CHECK |
| `body` | `BYTEA NOT NULL` | The item's exact original bytes, sliced from the request body with no reserialization |
| `body_sha256` | `BYTEA NOT NULL` | |
| `body_size` | `INTEGER NOT NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL` | Copied from the parent call (deliberate denormalization for type-scoped, time-ranged reads) |

**Constraints:**
- `call_payloads_ordinal_nonneg`: `ordinal >= 0`
- `call_payloads_sha_is_sha256`: `octet_length(body_sha256) = 32`
- `call_payloads_body_size_nonneg`: `body_size >= 0`
- `call_payloads_body_is_whole`: `octet_length(body) = body_size`
- `call_payloads_one_row_per_item`: UNIQUE `(call_id, ordinal)` — also the call FK's index and the ordered per-call read

**Indexes:**
- `idx_rc_call_payloads_type_created_at`: `(payload_type, created_at DESC)` (BTree)

---

### `remote_control.worker_status_transitions`

Added by `20260908000003_remote_control_session_link.sql`. One row per observed change in a
channel's `worker_status`, appended set-based inside `record_calls.sql` only when the incoming
status differs from what was stored — never on every heartbeat. `worker_status`/`worker_status_at`
on the channel row stay the current reading; this table is the history they cannot hold. Read by
`PersistHandle::fetch_worker_status_history` (`store/persistence/remote_control/reads.rs`; its
production caller is the flapping warning in `proxy::remote_control::capture::persist`) and the
session-flow status and cursor queries (`session_flow/status.rs`, `cursor.rs`). No REST or MCP
route reads it directly.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `channel_id` | `BIGINT NOT NULL` | FK → `remote_control.channels(id)` ON DELETE CASCADE |
| `from_status` | `TEXT NULL` | NULL for a channel's first observed status; open vocabulary, no CHECK |
| `to_status` | `TEXT NOT NULL` | |
| `observed_at` | `TIMESTAMPTZ NOT NULL` | The `worker_status_at` the transitioning call carried, not write time |
| `call_id` | `BIGINT NULL` | FK → `remote_control.calls(id)` ON DELETE SET NULL; the call that carried the status, when known |

**Indexes:**
- `idx_rc_worker_status_transitions_channel_observed`: `(channel_id, observed_at DESC)` (BTree)

---

### `remote_control.injections`

Added by `20260908000004_remote_control_injections.sql`; `source_kind`/`source_id` by
`20260916020000_push_wake_injection_sources.sql` ([issue #255](https://github.com/temet-ai/mike-private/issues/255));
`carried_request_id` by `20260923153500_injection_carried_request.sql`. The exact bytes of every
mike-authored, id-less SSE frame pushed into a session's live Remote Control reply stream, including
unsuccessful attempts: a database reservation (`reserve_injection.sql`,
`reserve_steerer_injection.sql`, `reserve_unclaimed_injection.sql`) and the source's claim precede
emission; `finish_injection.sql` stamps `emitted_at` or `failed_at`; only a captured
`POST /worker/events/delivery` with `status: "received"` and the same event UUID stamps `acked_at`
(`ack_injections.sql`, `ack_steering_interventions.sql`), which also consumes the mailbox row or
acks the intervention. `release_stale_mailbox_push.sql` returns an unacked mailbox push to ordinary
turn delivery. Once a later request body carries the acked reminder, `stamp_push_carriers.sql`
records it in `carried_request_id` and back-fills the source's `sent_request_id`. An ack proves
client receipt, not that the model acted on the message. See the prose above for the stream
mechanics.

| Column | Type | Purpose |
|---|---|---|
| `event_id` | `UUID` | Primary key; the event UUID the frame carries and the ack names |
| `channel_id` | `BIGINT NOT NULL` | FK → `remote_control.channels(id)` ON DELETE CASCADE |
| `message_uuid` | `UUID NULL` | FK → `mailbox.messages(message_uuid)` ON DELETE SET NULL; the pushed mailbox message, when the source is `mailbox` |
| `authored_by` | `TEXT NOT NULL DEFAULT 'mike'` | CHECK `authored_by = 'mike'` |
| `frame` | `BYTEA NOT NULL` | The exact SSE frame bytes |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Reservation time |
| `emitted_at` | `TIMESTAMPTZ NULL` | Handed to the reply stream |
| `failed_at` | `TIMESTAMPTZ NULL` | Emission failed or timed out; the claim is released |
| `acked_at` | `TIMESTAMPTZ NULL` | The addressed client reported `received`; first ack wins |
| `ack_call_id` | `BIGINT NULL` | FK → `remote_control.calls(id)` ON DELETE SET NULL; the delivery call that acked |
| `source_kind` | `TEXT NOT NULL DEFAULT 'mailbox'` | Which `ReminderSource` produced the push: `mailbox`, `steerer`, `lesson`, `diagnostic` or `recall` (no CHECK) |
| `source_id` | `TEXT NULL` | The source's own id as text (mailbox `message_uuid`, steering intervention uuid); NULL for a source with no owning row |
| `carried_request_id` | `BIGINT NULL` | The later request whose body carried the acked reminder (no FK); NULL until stamped |

**Indexes:**
- `idx_rc_injections_channel`: `channel_id` (BTree)
- `idx_rc_injections_message`: `message_uuid WHERE message_uuid IS NOT NULL` (BTree partial)
- `idx_rc_injections_ack_call`: `ack_call_id WHERE ack_call_id IS NOT NULL` (BTree partial)
- `idx_rc_injections_source`: `(source_kind, source_id) WHERE source_id IS NOT NULL` (BTree partial) — ack routing by source
- `idx_rc_injections_uncarried`: `channel_id WHERE acked_at IS NOT NULL AND carried_request_id IS NULL` (BTree partial) — acked pushes awaiting their carrier request

---






## Domain: `workspace`

Added in migration `20260707000001_workspace_core.sql` ([issue #148](https://github.com/temet-ai/mike-private/issues/148), session-coordination Phase 1).
Registration/liveness + the durable coordination-event ledger. Deliberately out of this migration (later
phases): `area_claims`, locks, `project_docs`, `agent_queries`, NOTIFY triggers, and any
`events_retention_days` settings column — Phase 1's roster is assembled handler-side over the two tables
below; no read view exists yet.

### `workspace.session_registrations`

One row per live/recent hook registration — hook/passive session presence with a liveness lease. PK = FK on
`session_id` (mirrors the `session_goals` pattern): a registration is 1:1 with a session and meaningless
without it, so it CASCADEs; `project_id` CASCADEs too, matching `sessions.sessions.project_id`'s own cascade.
No `intent` column — the register handler upserts `sessions.session_goals` instead when the hook supplies
one, rather than maintaining two competing "what is this session doing" declarations.

`lease_expires_at` is the liveness lease every roster query reads (`now() < lease_expires_at`); Phase 1 has
no expiry worker, so a stale row that is never explicitly ended simply reads as expired — read-computed, not
swept, since nothing (no claim, no lock) yet depends on it. **A lapsed lease means the session's state is
UNKNOWN, not gone** — it may be mid-burst on real traffic and simply never renewed its heartbeat, or it may
genuinely be dead; `stale` is never a synonym for `ended`, which only ever comes from an explicit `end` call.

The roster query (`fetch_workspace_roster`) additionally reads each row's most recent
`requests.requests` activity via a `LEFT JOIN LATERAL` on `idx_requests_session_created_id`
(`last_request_at`, `newest_request_number`; issue temet-ai/mike-private#129) — read-time activity truth from captured
traffic, not a stored column, `NULL` for a registration with zero captured requests. The default
(`Live`-scope) roster keeps any non-ended row whose activity lands within `HEARTBEAT_LEASE_MINUTES`
even once its own lease has lapsed, so a session generating real traffic never drops off the default
roster for want of a heartbeat; `liveness` itself is untouched by this — a row kept alive by activity
alone still reads `stale`.

**Channel-derived liveness** (issues temet-ai/mike-private#231/#234, migration `20260908000003_remote_control_session_link.sql`):
`fetch_workspace_roster` runs a second query, `fetch_channel_snapshots` (see the `remote_control`
domain below), against every roster row's session uuid and merges the result in
(`store::persistence::workspace::roster::merge_channel_snapshot`) — not a JOIN in the lease query,
since a lease and a channel are independent facts and a session can have either without the other.
When a session has a linked Remote Control channel, `liveness` is recomputed from the channel's
heartbeat recency (`heartbeat_age_seconds <= 3 * max(heartbeat_interval_seconds, 20)`, roughly a
minute at the ~20s cadence every observed client uses) instead of the registration lease, and
`liveness_source` reads `"channel"`; otherwise `liveness_source` reads `"lease"` and the original
lease-only rule applies unchanged. An explicit `Ended` (from `released_at`) always wins over either
source. This is how the roster answers "is the process actually still there" within about a minute of
a killed process, rather than waiting up to `HEARTBEAT_LEASE_MINUTES` for the lease to lapse. The
merge adds these fields to every roster row, all `None`/`false` when `channel_present` is `false`:
`channel_present`, `channel_cse_id`, `heartbeat_age_seconds`, `heartbeat_interval_seconds`,
`idle_seconds`, `session_state` (the channel's `worker_status` verbatim), `session_state_at`,
`reply_channel_open` (the inbound SSE stream), `reply_channel_opened_at`, `last_inbound_message_at`,
`post_turn_summary`, `post_turn_summary_at`, `client_version`.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `BIGINT PRIMARY KEY` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `cwd` | `TEXT NOT NULL` | Raw, verbatim cwd string from the hook payload — a client-side signal, not validated against `project_id` |
| `worktree` | `TEXT NULL` | The `.claude/worktrees/<name>` segment when the hook fires inside a worktree checkout; NULL for a main-checkout session |
| `registered_via` | `TEXT NOT NULL DEFAULT 'hook'` | CHECK `IN ('hook', 'mcp', 'rest', 'passive', 'operator')`. `passive` is already a first-class value for Phase 2's back-fill (scope-rebalance R5/Q5), not a later ALTER |
| `registered_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `lease_expires_at` | `TIMESTAMPTZ NOT NULL` | Liveness lease; read-computed (`now() < lease_expires_at`), no Phase 1 expiry worker |
| `released_at` | `TIMESTAMPTZ NULL` | Explicit deregistration (register/end route). NULL is the common case — most registrations end by lease expiry, not an explicit call |
| `pid` | `INTEGER NULL` | OS process id of the registering session, when supplied. NULL rows are never reapable — pids recycle, so no pid means no liveness check |
| `boot_id` | `TEXT NULL` | Server-derived boot identifier (`kern.boottime` / `/proc/stat btime`) captured at register time, never client-supplied. A row whose `boot_id` predates the sweeper's current boot is never reapable, since its pid may have been recycled since |
| `reaped_at` | `TIMESTAMPTZ NULL` | Set alongside `released_at` only when the staleness sweeper closed this row because its process was gone. NULL for a clean explicit end — the distinguishing signal between reaped and ended |

**Indexes:**
- `idx_session_registrations_project_lease`: `(project_id, lease_expires_at)` (BTree) — matches the roster query shape `WHERE project_id = $1 AND lease_expires_at > now()`
- `idx_session_registrations_reapable`: `boot_id WHERE released_at IS NULL AND pid IS NOT NULL` (BTree partial) — staleness sweeper's per-boot scan

**Heartbeat** ([issue #168](https://github.com/temet-ai/mike-private/issues/168)): `POST /api/workspace/heartbeat`
(`{session_id}`) renews `lease_expires_at` on the caller's active registration and returns
`mailbox.messages`' `unconsumed`/`unclaimed` inbox counts for that session in the same round-trip
(one query per count, no correlated scalars) — `404` when the session has no active (non-released)
registration. The `workspace-heartbeat` Stop hook is the sole caller: it renews the lease on every
turn boundary and blocks termination while `unconsumed > 0`, converging the instant the recipient
acks (or the message is dead-lettered).

**Project-identity unification** ([issue #148](https://github.com/temet-ai/mike-private/issues/148)):
registration keys `project_id` on the same trimmed display form captured requests already resolve
to (`extract::request::project_display_name`, applied to the hook's cwd), not the caller's absolute
path — so a repo has exactly one `projects.projects` row regardless of whether it was first seen via
registration or via traffic. `find_project_id_by_path` (backs the roster route) exact-matches first;
a leading-`/` input that misses falls back to its own canonicalization (the same trimmed-form
derivation) so a caller still passing a raw absolute cwd resolves to the one unified row instead of
404ing. Migration `20260712000003_workspace_project_identity_unification` reconciles rows created
under the pre-fix behavior (absolute-path rows renamed in place, or merged into a pre-existing
trimmed-form row when both existed).

### `workspace.event_kinds` (vocab)

Seeded: `registered` (1), `ended` (2), `expired` (3), `note` (4). `bigint identity` + `code` matches every
other vocab table in this repo (`rules.kinds`, `steering.authorities`/`modes`/`verdicts`,
`session_judges.outcomes`). The vocabulary grows in later phases: `claimed`/`released`/`conflict` arrive with
Phase 3 (claims/locks), `lock_acquired`/`lock_released`/`lock_expired`/`lock_stolen`/`lock_violation` with
Phase 4. `shipped` is cut outright — ship/correction outcomes already live in `memory.operator_turns`, not here.

### `workspace.coordination_events`

Append-only "who did what" ledger — durable rows that must survive the session TTL sweep, so
`session_id` FKs `ON DELETE SET NULL` rather than `CASCADE`. `actor_label` is denormalized at write time
(the full session UUID plus title, never a truncated prefix) to carry attribution across that sweep — after
the sweep it is the only surviving attribution, and prefixing would be a permanent, unrecoverable truncation
for zero space saved (the exact pattern `agent-behavior-and-data.md` bans).

`detail` is genuinely schemaless: its shape varies per event kind (a conflict event's peer/prefix pair looks
nothing like a lock event's token/expiry pair), and every fact the roster/board queries actually filter or
sort on (`project_id`, `session_id`, `kind_id`, `created_at`) is already its own typed column beside this JSONB.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL |
| `actor_label` | `TEXT NOT NULL` | Full session UUID + title denormalized at write time — never a prefix. The only surviving attribution once `session_id` goes NULL after the TTL sweep |
| `kind_id` | `BIGINT NOT NULL` | FK → `workspace.event_kinds(id)` ON DELETE RESTRICT |
| `subject` | `TEXT NOT NULL` | Free-text ledger subject: session UUID for `registered`/`ended`/`expired`, or the reconciliation detail for `note`. Lock name and claim prefix become subjects in later phases |
| `detail` | `JSONB NOT NULL DEFAULT '{}'` | Schemaless per-kind payload |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

**Indexes:**
- `idx_coordination_events_project`: `(project_id, id DESC)` (BTree) — board/roster read: events for one project, most recent first
- `idx_coordination_events_session`: `session_id` (BTree)
- `idx_coordination_events_kind`: `kind_id` (BTree)
- `brin_coordination_events_created`: `created_at` BRIN — append-mostly time series, repo convention for this workload shape

### `workspace.session_notes`

Added in migration `20260920000002_session_notes.sql`. A session's own **notebook**: the notes, findings,
tasks and decisions it records for itself and for peers to retrieve. Many-per-session, unlike the 1:1
`session_goals` identity row.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGSERIAL PRIMARY KEY` | Never leaves the store layer |
| `note_uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | The stable external handle the REST and MCP surface resolves by, matching the `message_uuid`/`session_uuid` convention elsewhere in this schema |
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE — the authoring session |
| `kind` | `TEXT NOT NULL DEFAULT 'note'` | Free text, open set, no CHECK. Conventional values: `note`, `finding`, `task`, `decision`, `question` |
| `title` | `TEXT NOT NULL` | |
| `body` | `TEXT NOT NULL DEFAULT ''` | |
| `status` | `TEXT NULL` | Free text, open set. NULL for a kind with no status concept. Conventional task values: `open`, `in_progress`, `blocked`, `done` |
| `tags` | `TEXT[] NOT NULL DEFAULT '{}'` | |
| `data` | `JSONB NOT NULL DEFAULT '{}'` | The free-form payload each session shapes for itself — genuinely schemaless from the store's point of view |
| `created_at` / `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` each | |

**Row per entry, never a JSON array blob per session.** The operator's ask was for "a json structure in a
table they can edit as they need" — that is `data`. The surrounding typed columns exist because the point of
the notebook is *retrieval*: filtering by kind/status/tag, ordering and paging over a single blob column
would mean `jsonb_array_elements` on every read, discarding every index the table could otherwise carry.
The rule the schema follows: anything a real query filters or sorts on is its own column; `data` is for
structure the notebook itself does not need to interpret.

`kind` and `status` are free text with no CHECK, for the same reason `sessions.session_goals.role` is —
the vocabulary is declared by the sessions that use it, not fixed by the schema.

**No `project_id` column.** Project identity lives on `sessions.sessions` and is repointed by
`heal_project_assignment` / `assign_session_project`; a denormalised copy here would drift the moment a
session's project was reassigned after the note was written. The cross-session query
(`fetch_project_notes` — "what did the observer sessions find", a join to `session_goals.role`) goes
through `sessions.sessions` instead.

**Indexes:**
- `idx_session_notes_session_created`: `(session_id, created_at DESC)` — a session's own notes newest-first, and the FK column's own lookup
- `idx_session_notes_note_uuid` (UNIQUE): `note_uuid` — the external handle every read/update/delete resolves by
- `idx_session_notes_tags`: `tags` GIN — tag filtering is array containment (`tags @> ARRAY[$1]`), which a BTree cannot serve at all
- `idx_session_notes_data`: `data` GIN `jsonb_path_ops` — smaller and faster for containment than the default `jsonb_ops`, at the cost of the key-existence operators (`?`, `?|`, `?&`) no current query uses

### `workspace.change_events`

One record of what happened in a checkout: file changes, commits, pushes, checks, verdicts, notes, mail and
briefs, each with the paths it concerns and, where known, the session that made it. Written only while the
`change_record` toggle is on. Reference: [`CHANGE_RECORD.md`](CHANGE_RECORD.md). Added in migration
`20261005030000_change_record.sql`, which only adds the two tables and their indexes (and the two toggle names
to the `config.feature_toggles` key check), so a server on an older build that shares this database is
unaffected. `kind` has a CHECK on the ten values below.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGSERIAL PRIMARY KEY` | The order events are read in, and what a session's place points at |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `checkout` | `TEXT NOT NULL` | The repository root on the machine that recorded the event |
| `at` | `TIMESTAMPTZ NOT NULL` | When the event happened |
| `kind` | `TEXT NOT NULL` | `edit`, `shell_change`, `commit`, `push`, `check`, `verdict`, `note`, `mail`, `brief` or `task_state` |
| `session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; the session that made it, NULL when none can be named |
| `run_id` | `TEXT NULL` | The fleet run, for a seat of a fleet |
| `task` | `TEXT NULL` | The fleet task, for a seat of a fleet |
| `paths` | `TEXT[] NOT NULL` | The files the event concerns, relative to the checkout |
| `subject` | `TEXT NOT NULL` | One line: a commit subject, a note title, the first line of a mail |
| `detail` | `JSONB NOT NULL` | Per-kind payload: the tool_use id of an edit, the candidates when no author could be named, a note's or message's id |
| `before_sha` / `after_sha` | `TEXT NULL` | Git blob ids of a file's content before and after a change; the content itself is in the repository's object store |

**Indexes:**
- `(checkout, id)` (BTree): every read is one checkout's events after a given id
- `paths` GIN: finding the events that name a file or a folder
- `project_id` (BTree) and `session_id` (partial, where not NULL): the foreign keys' own lookups

There is deliberately no index on a sha column.

### `workspace.change_places`

For each session and checkout, the last change event the session was shown. "What changed since I last
looked" is the rows of `change_events` with a larger `id`.

| Column | Type | Purpose |
|---|---|---|
| `session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `checkout` | `TEXT NOT NULL` | The checkout |
| `last_event_id` | `BIGINT NOT NULL` | The newest event the session has been shown |
| `updated_at` | `TIMESTAMPTZ` | When the place last moved |

**Primary key:** `(session_id, checkout)`.

### `workspace.questions`

A question one session asked another, with a state. Added by migration
`20261006120000_workspace_questions.sql`, which only adds the table and its indexes. The
`ask` tool writes a row and mails the recipient once; `answer` and `decline` settle it
(`UPDATE ... WHERE state = 'open'`, so one settle wins and the asker is mailed once). It stays in the
recipient's `what_changed` view `standing` under `questions` until settled. A blocked question open for
five minutes is claimed by one statement that sets `escalated_at` and is mailed once to the fleet's
reference seat.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key; never leaves the store layer |
| `question_uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | UNIQUE. The id in mail and tool answers |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE; the asker's project |
| `run_id` | `TEXT NULL` | FK → `launch.runs(run_id)` ON DELETE SET NULL; the asker's fleet run, NULL outside a fleet |
| `asker_session_id` | `BIGINT NOT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE |
| `to_session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE CASCADE; the seat asked |
| `to_task` | `TEXT NULL` | The task the question was addressed to (`task:<id>`), kept as asked; CHECK that this or `to_session_id` is set |
| `about_note_uuid` | `UUID NULL` | FK → `workspace.session_notes(note_uuid)` ON DELETE SET NULL; the note (a brief part, a finding) the question is about |
| `question` | `TEXT NOT NULL` | The question, whole; CHECK not blank |
| `blocked` | `BOOLEAN NOT NULL DEFAULT false` | The asker cannot go on without the answer |
| `state` | `TEXT NOT NULL DEFAULT 'open'` | CHECK `IN ('open', 'answered', 'declined')`; `settled_at IS NULL` exactly when open; an open row has no answer |
| `answer` | `TEXT NULL` | The answer, or the reason for a decline |
| `settled_by_session_id` | `BIGINT NULL` | FK → `sessions.sessions(id)` ON DELETE SET NULL; anyone in the project may settle |
| `created_at` / `settled_at` / `escalated_at` | `TIMESTAMPTZ` | Asked, settled (NULL while open), and handed to the reference seat (NULL until then; the once-only mark) |

**Indexes:** `(to_session_id, state)` (a session's open questions), `(run_id, state)` (a run's open questions and the
escalation sweep), and one on each remaining foreign key (`project_id`, `asker_session_id`,
`settled_by_session_id` and `about_note_uuid`, the last two partial where not NULL).

---

---

## Domain: `launch`

Added in migrations `20260916000001_launch_domain.sql` and
`20260916030000_launch_launches_row_changed_trigger.sql` ([issue #257](https://github.com/temet-ai/mike-private/issues/257),
folding a standalone terminal-launcher process into mike as a native launch domain). Four
tables: `templates` (warm sessions `mike spawn fork` starts from), `runs` (a run id's shared
backend workspace/tab), `launches` (one row per launch attempt, whatever its outcome),
`backend_validations` (every live terminal-backend validation, whoever triggered it). Later
migrations added `fleets`, `told`, `run_records`, `fleet_events`, `tree_changes` (what changed in a
run's checkout), `tree_checks` (the project's check command, run once per tree state) and
`introductions` (seats that repeated another seat's slow command), all documented below. See
[`LAUNCH_DOMAIN.md`](LAUNCH_DOMAIN.md) for the CLI/MCP surface these tables back.

### `launch.templates`

One live row per template name; a re-warm (`mike spawn warm` under a name already in use)
supersedes the previous row by pointing it at the new one rather than rewriting it, so forks
already taken from the superseded template keep a `template_id` that still resolves.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `name` | `TEXT NOT NULL` | The discipline/name a fork resolves; unique only among live (non-superseded, non-closed) rows |
| `session_uuid` | `UUID NOT NULL` | The template session's own id, preassigned by the launcher; a fork passes it as `--resume <session_uuid> --fork-session` |
| `folder` | `TEXT NOT NULL` | |
| `backend` | `TEXT NOT NULL` | |
| `model` | `TEXT NULL` | |
| `agent` | `TEXT NULL` | |
| `claude_args` | `JSONB NOT NULL` | The complete claude argv the template was warmed with, in order — JSONB because it is an ordered argv, not a set of facts anything filters on. A fork prepends its own resume flags to exactly this list, and refuses a prefix change without an explicit opt-in |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `superseded_by` | `BIGINT NULL` | FK → `launch.templates(id)` ON DELETE SET NULL. The template that replaced this one under the same name; NULL while this row is the live template for its name |
| `closed_at` | `TIMESTAMPTZ NULL` | |

**Indexes:**
- `idx_launch_templates_live_name`: `name WHERE superseded_by IS NULL AND closed_at IS NULL` (BTree unique partial) — matches the fork lookup exactly: one live template per name
- `idx_launch_templates_superseded_by`: `superseded_by WHERE superseded_by IS NOT NULL` (BTree partial) — the self-referencing FK's own lookup and the `ON DELETE SET NULL` walk

### `launch.runs`

A run groups launches sharing a `--run <id>` into one tab of the project's workspace where the
backend can place them; the first launch carrying a run id creates the tab and records it here,
later launches under the same id split panes inside it.

| Column | Type | Purpose |
|---|---|---|
| `run_id` | `TEXT PRIMARY KEY` | |
| `project_path` | `TEXT NULL` | |
| `backend` | `TEXT NOT NULL` | |
| `workspace_ref` | `TEXT NULL` | The backend's opaque handle for the workspace. NULL on a backend without workspace placement |
| `tab_ref` | `TEXT NULL` | The backend's opaque handle for the tab. NULL when the backend has no tab placement; the run still exists so `list`/`kill --run` can group by it |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `closed_at` | `TIMESTAMPTZ NULL` | |

### `launch.launches`

One row per launch attempt by mike's launcher, whatever the outcome. The row is written when
the attempt starts, updated as the backend reports the surface open, when the outcome is known,
and when the surface is closed. Never deleted — it is the record of what a session was told and
where it ran, including the full claude argv and the full brief text — never a slice, since the
brief is the one thing a launched session was told that a captured transcript can fail to show.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `session_uuid` | `UUID NULL` | Preassigned by the launcher so the roster and the channel check can find the session before its first request. NULL for a `--resume`/`--continue` launch without an explicit `--session-id` |
| `backend` | `TEXT NOT NULL` | CHECK `IN ('herdr', 'tmux', 'terminal', 'ghostty', 'none')` |
| `folder` | `TEXT NOT NULL` | |
| `repo_root` | `TEXT NULL` | |
| `worktree` | `BOOLEAN NOT NULL DEFAULT FALSE` | |
| `workspace_ref` / `tab_ref` / `surface_ref` / `pid` | `TEXT` / `TEXT` / `TEXT` / `INTEGER`, all `NULL` | Backend handles, opaque to mike (herdr workspace/pane ids, a tmux session name, a terminal's pid). Filled in once the backend reports the surface open. `surface_ref` is what `kill` uses to find the surface again |
| `run_id` | `TEXT NULL` | FK → `launch.runs(run_id)` ON DELETE SET NULL — a run going away must not erase the record that a session was launched |
| `seat` | `TEXT NULL` | |
| `template_id` | `BIGINT NULL` | FK → `launch.templates(id)` ON DELETE SET NULL |
| `model` / `agent` | `TEXT NULL` each | Promoted out of `claude_args` because these are what an operator lists by |
| `claude_args` | `JSONB NOT NULL DEFAULT '[]'` | The full claude argv after mike's own flags, in order — complete so a launch can be reproduced and a fork can compare prefixes |
| `prompt_present` | `BOOLEAN NOT NULL DEFAULT FALSE` | |
| `brief_mode` | `TEXT NULL` | CHECK `IN ('inline', 'file')`. How the brief reached claude; NULL when there was no brief |
| `brief_path` | `TEXT NULL` | |
| `brief_text` | `TEXT NULL` | The whole brief as handed to claude, with `{{session_id}}` already substituted. Never a slice |
| `requested_placement` / `actual_placement` / `split_direction` / `anchor_ref` | `TEXT NULL` each | Placement vocabulary is the backend's own (tab/panel/window and split directions differ per backend), so these stay open text rather than a CHECK that would have to know every backend |
| `remote_control` | `BOOLEAN NOT NULL DEFAULT TRUE` | |
| `proxy_env` | `BOOLEAN NOT NULL DEFAULT TRUE` | |
| `channel_check` | `TEXT NULL` | CHECK `IN ('found', 'timeout', 'db_unreachable', 'no_session_id', 'disabled')`. NULL until the wait for a Remote Control channel has run; `found` is the only pass |
| `channel_checked_at` | `TIMESTAMPTZ NULL` | |
| `opened` | `BOOLEAN NOT NULL DEFAULT FALSE` | |
| `delivered` | `BOOLEAN NULL` | Whether the brief was delivered to the surface. NULL means there was nothing to deliver, which is not the same as a failed delivery |
| `confirmed` | `BOOLEAN NOT NULL DEFAULT FALSE` | |
| `result` | `TEXT NOT NULL DEFAULT 'pending'` | CHECK `IN ('pending', 'success', 'unconfirmed', 'failed')` |
| `reason` | `TEXT NULL` | |
| `stderr_text` | `TEXT NULL` | Bounded capture of a failed backend call's stderr, at most 64 KiB — the only truncation this schema sanctions, since stderr is diagnostic noise of unbounded size, not inspection content |
| `stderr_truncated` | `BOOLEAN NOT NULL DEFAULT FALSE` | Whether more than 64 KiB was cut |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `opened_at` / `closed_at` | `TIMESTAMPTZ NULL` each | |
| `close_reason` | `TEXT NULL` | |
| `close_process_exited` | `BOOLEAN NULL` | Whether the claude process had exited by the time the surface was closed. NULL when the backend could not tell |
| `identity` | `JSONB NULL` | Added in `20260920000003_launch_identity.sql`. Spawn-time identity handoff for the preassigned `session_uuid`: `{"name", "role", "instructions", "goal", "next_steps"}`, every key optional. Read once by workspace registration and applied to `sessions.session_goals`, never re-applied over a value the session has since declared for itself. One JSONB column rather than four text columns on an already forty-column table because nothing sorts, groups or searches launches by these — the only lookup is "the launch row for this session_uuid", already served by `idx_launch_launches_session_uuid` |

**Indexes:**
- `idx_launch_launches_session_uuid`: `session_uuid WHERE session_uuid IS NOT NULL` (BTree partial) — `kill --session` and the roster join
- `idx_launch_launches_run_created`: `(run_id, created_at) WHERE run_id IS NOT NULL` (BTree partial) — `list`/`kill --run`, and the run FK's own lookup
- `idx_launch_launches_template_id`: `template_id WHERE template_id IS NOT NULL` (BTree partial) — the template FK's lookup
- `idx_launch_launches_created_at`: `created_at DESC` (BTree) — newest launches first for `mike spawn list` and the Launches panel
- `idx_launch_launches_backend_open`: `(backend, closed_at) WHERE closed_at IS NULL` (BTree partial) — the live-launches-per-backend count behind the surface cap (`launch_warn_surfaces`/`launch_max_surfaces`)

**NOTIFY trigger:** `trg_launch_launches_row_changed` fires `mike.notify_row_changed_launch_launches()` on every insert/update/delete, publishing `{table: "launch_launches", id, session_id, op}` on `proxy.row_changed` — the same channel every other live-updating table uses to reach the SPA and any subscribed WebSocket client, since `POST /api/launches` returns before a launch's progress (opened, channel-checked, closed) is known.

**Added to `launch.launches` by `20261006130000_launch_sibling_wait.sql`:** `sibling_wait_seconds INTEGER NULL` (CHECK ≥ 0) and `waited_for_seat TEXT NULL`: how long a fork waited for a sibling fork of the same template to start first, and which seat it waited for. NULL when it did not wait.

### `launch.told`

Added in `20261006110000_fleet_told_ledger.sql`. mike's once-only record of the notices it sends a run by itself: one row per `(run_id, what)` (the primary key), with `told_at`. The fleet tick reads it before it mails, so a restart does not send a notice again.

### `launch.run_records`

Added in `20261006100000_fleet_run_records.sql`. One row per fleet, written once when the run closes by `launch::fleet::run_record::close_record` from set-based queries over `launch.launches`, `requests.requests`, `requests.tools`, `tools.uses` and `mailbox.messages`. `fleet_uuid` and `run_id` are unique, with FKs to `launch.fleets` and `launch.runs` (ON DELETE CASCADE); `project_id` references `projects.projects` (SET NULL) and is indexed. The measures are typed columns (`working_minutes`, `waiting_minutes`, `first_commit_median_minutes`, `report_to_verdict_median_minutes`, `worker_git_reads`, `git_reads_per_100_calls`, `cargo_commands`, `cargo_repeats_across_seats`, `questions_asked`, `questions_answered`, `forks`, `forks_waited`, and the counts beside them). `seats`, `mail_by_pair`, `tools_offered` and `tools_used` are JSONB display documents read whole by `GET /api/fleets/{fleet}/record`; nothing filters on them. `measure_version` names the queries.

**Added by `20261022000100_run_records_v3.sql`** (run record v3): the four numbers a run is scored on, as typed nullable columns, NULL on records written before v3: `time_to_done_minutes` (`double precision`, first seat launch to last seat close), `fault_minutes_total` (`double precision`, the sum of the parts in `score.fault_minutes`), `tokens_per_accepted_task` (`double precision`, every request's input, output, cache-read and cache-write tokens divided by the tasks that had a worker seat) and `manual_interventions_total` (`bigint`, operator turns typed into seats, hand-started seats and answered operator notes). `score` (`jsonb`) is the explanation document: the fault-minute parts, tokens by seat kind, the manual-intervention parts and the counts that explain them. It is read whole by `GET /api/fleets/{fleet}/record`; nothing filters on a field inside it.

### `launch.fleet_events`

Added in `20261022000000_launch_fleet_events.sql`. The append-only record of a fleet run: one row per thing that happened, never updated, so every state a fleet shows is a fold over a run's rows in `id` order. `id` is `BIGINT GENERATED ALWAYS AS IDENTITY` (PK); `run_id` is `TEXT NOT NULL`, FK → `launch.runs(run_id)` ON DELETE CASCADE; `task` (`TEXT NULL`) and `session_uuid` (`UUID NULL`) name what the event is about, both NULL for an event about the whole run; `kind` is `TEXT NOT NULL` with a CHECK listing the kinds in [`fleets/events.md`](fleets/events.md); `payload` is `JSONB NOT NULL DEFAULT '{}'`, decoded whole in Rust by kind, carrying ids and shas and never captured text; `occurred_at` is `TIMESTAMPTZ NOT NULL DEFAULT now()`. Indexes: `(run_id, id)`, a partial `(run_id, task, kind) WHERE task IS NOT NULL`, and `(session_uuid, kind)` (`idx_fleet_events_session_kind`, `20261109130001`; a seat's events by kind, read by `memory.seat_memory`). Fleet 2's writers fill it, listed per task in [`fleets/events.md`](fleets/events.md) (the fold reads it for every shown state); the persistence calls are `append`, `since(run, after_id)` and `for_task(run, task)`.

### `memory.carried_lessons`

Added in the same migration, with the `carry` row of `memory.lesson_sources`. One row per session note marked `data.carry` that became a lesson candidate: `lesson_id` (PK, FK to `memory.lessons`, cascade), `project_id` (FK, cascade), `discipline`, `tag` (`carry:<slug of the title>`), `source_note_uuid` (unique) and the two counts, `runs_given` and `later_findings`, which only the two bridge tables move: `memory.carried_lesson_runs (lesson_id, run_id)` gets a row when a template is warmed with the lesson, and `memory.carried_lesson_findings (lesson_id, note_uuid)` when a later note carries its tag. Index `(project_id, discipline, tag)`.

### `memory.carried_lesson_runs`

Added in `20261006100000_fleet_run_records.sql`. One row per `(carried lesson, fleet run)` the lesson was
given to: `store/persistence/fleet/carry_sql/warm.sql` inserts a row for each unarchived `daily` or
`long_term` carried lesson of the fleet's project and requested disciplines when a template is warmed, and
increments `memory.carried_lessons.runs_given` once per new row.

| Column | Type | Purpose |
|---|---|---|
| `lesson_id` | `BIGINT NOT NULL` | FK → `memory.carried_lessons(lesson_id)` ON DELETE CASCADE |
| `run_id` | `TEXT NOT NULL` | The fleet run; no FK to `launch.runs` |
| `given_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Primary key:** `(lesson_id, run_id)`.

---

### `memory.carried_lesson_findings`

Added in `20261006100000_fleet_run_records.sql`. One row per `(carried lesson, later session note)` whose
tag matched the lesson's `carry:<slug>` tag: `store/persistence/fleet/carry_sql/count_findings.sql` (and
`count_findings_discipline.sql`) inserts a row when a note other than the lesson's own source note, written
after the lesson, carries its tag, and increments `memory.carried_lessons.later_findings` once per new row.

| Column | Type | Purpose |
|---|---|---|
| `lesson_id` | `BIGINT NOT NULL` | FK → `memory.carried_lessons(lesson_id)` ON DELETE CASCADE |
| `note_uuid` | `UUID NOT NULL` | The later note; no FK to `workspace.session_notes` |
| `found_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Primary key:** `(lesson_id, note_uuid)`.

---



### `launch.backend_validations`

The newest row per backend is what setup and `/settings` show as the backend's last known
state; older rows are the history of when a backend stopped or started working.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `backend` | `TEXT NOT NULL` | |
| `passed` | `BOOLEAN NOT NULL` | |
| `version` | `TEXT NULL` | |
| `protocol` | `TEXT NULL` | |
| `checks` | `JSONB NOT NULL` | The sub-check report in run order — an array of `{name, passed, detail, remedy}`. Displayed whole; `passed` on the row is the queried verdict |
| `started_at` / `finished_at` | `TIMESTAMPTZ NOT NULL` each | |
| `source` | `TEXT NOT NULL` | CHECK `IN ('setup', 'doctor', 'settings', 'launch')` — what triggered the validation |

**Indexes:**
- `idx_launch_backend_validations_backend_finished`: `(backend, finished_at DESC)` (BTree) — "the latest validation for this backend" is the only read

**Registration:** the launcher registers a launched session on `workspace.session_registrations`
the moment the backend reports the surface open, under `registered_via = 'launcher'` (added to
that column's CHECK constraint by this same migration) — before the session's own first
captured request.

### `launch.fleets`

Added in `20260930000000_launch_fleets.sql`. A fleet definition and its progress: the seats,
agents, models and criteria an operator (or a session on their behalf) has described, whether
the checks pass, and which run the start launched. One row per fleet; `state` moves
`draft`, `ready`, `starting`, `running`, then `done` or `failed`, through `stopping`
(added by `20261014070100_fleet_stopping.sql`) while a stop fences new work and the launch's
managed command receipts settle. Read and written through
`store/persistence/fleet` (`insert_fleet`, `find_fleet`, `list_fleets`, `save_fleet_definition`,
`set_fleet_state`), which also holds the two status reads over a fleet's run
(`fleet_seat_rows`, `fleet_note_rows`) — those read `launch.launches`, `sessions.*`,
`requests.*`, `responses.responses` and `workspace.session_notes` and add no columns to them.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key; never leaves the store layer |
| `fleet_uuid` | `UUID NOT NULL DEFAULT gen_random_uuid()` | UNIQUE. The external handle every route and command resolves by |
| `slug` | `TEXT NOT NULL` | A short name the operator types. Not unique: a slug resolves to the newest fleet carrying it |
| `project_path` | `TEXT NOT NULL` | |
| `state` | `TEXT NOT NULL DEFAULT 'draft'` | CHECK `IN ('draft', 'ready', 'starting', 'running', 'stopping', 'failed', 'done')` |
| `definition` | `JSONB NOT NULL DEFAULT '{}'` | The definition document. JSONB because the fleet code validates it whole in Rust and nothing filters or joins on a field inside it |
| `missing` | `JSONB NOT NULL DEFAULT '[]'` | The last check result, an array of `{item, problem}`; empty means every check passed. Replaced whole on each save |
| `run_id` | `TEXT NULL` | FK → `launch.runs(run_id)` ON DELETE SET NULL — a run going away must not erase the fleet that started it. NULL until the start names a run |
| `start_report` | `JSONB NULL` | One entry per seat the start launched. NULL until a start has run |
| `created_by` | `UUID NULL` | The session that created the fleet. Deliberately not a foreign key: that session may have no `sessions.sessions` row yet, and the fleet must outlive it |
| `created_at` / `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | `updated_at` is bumped on every write and is the compare value of `save_fleet_definition`: it writes only while `updated_at` still equals the value the writer read, so two writers cannot silently overwrite each other |
| `ready_at` / `started_at` / `finished_at` | `TIMESTAMPTZ NULL` each | Stamped by `set_fleet_state` when the state becomes `ready`, `running`, and `failed` or `done`; replaced on each later entry to that state |

**Indexes:**
- `idx_launch_fleets_project_created`: `(project_path, created_at DESC)` (BTree) — the per-project listing, newest first
- `idx_launch_fleets_slug_created`: `(slug, created_at DESC)` (BTree) — slug lookup, newest wins
- `idx_launch_fleets_run_id`: `run_id WHERE run_id IS NOT NULL` (BTree partial) — the run FK's own lookup and the `ON DELETE SET NULL` walk

---

### `launch.tree_changes`

Added by `20260930020000_launch_tree_changes.sql`. What changed in a fleet run's checkout, read
from the tree itself once a second while the run is running (`launch/fleet/watch.rs`, `git status
--porcelain=v2`): one row per path per observed change, with the blob hash of the new content. Who
made the change is worked out at read time from the seats' tool calls in flight at `seen_at`.
Written by `fleet/insert_tree_changes.sql`; read by `tree_changes_since.sql` and
`tree_changed_paths_since.sql` for `GET /api/fleets/{fleet}/changes` (`api/fleet/changes.rs`),
the fleet tree view (`api/fleet/tree.rs`) and the fleet tick's introductions, proposals, floor and
report proof (`launch/fleet/introduce.rs`, `propose_tick.rs`, `floor.rs`, `report_proof.rs`,
`reuse.rs`). `run_id` is a soft reference to `launch.runs(run_id)`: no FK.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `run_id` | `TEXT NOT NULL` | The fleet run (soft reference) |
| `path` | `TEXT NOT NULL` | Path relative to the checkout |
| `change` | `TEXT NOT NULL` | CHECK `IN ('added', 'modified', 'deleted')` |
| `content_sha` | `TEXT NULL` | Blob hash of the new content; NULL for a deletion |
| `seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_tree_changes_run_seen`: `(run_id, seen_at, id)` (BTree)
- `idx_tree_changes_run_path`: `(run_id, path, seen_at)` (BTree)

---

### `launch.tree_checks`

Added by `20261001010000_launch_tree_checks.sql`; `state_after` by
`20261001030000_tree_checks_state_after.sql`, `executing_at` by
`20261101130000_tree_check_executing_at.sql`. The project's declared check command, run by mike for
a fleet run once per state of the tree: `state` names the content of every file under the check's
declared paths, so a second seat asking at the same state gets this row instead of a second run.
The whole output is kept. Written and read through `store/persistence/fleet/` (`begin_tree_check`,
`find_tree_check`, `latest_tree_check`, `add_tree_check_asker`, `add_tree_check_told`,
`mark_tree_check_executing`, `finish_tree_check`, `abandon_expired_checks`, `abandon_unrun_check`,
`measured_tree_check_seconds`) by the fleet's check, gate and commit-verify paths
(`launch/fleet/check_run.rs`, `check_completion.rs`, `gate.rs`, `gate_queue.rs`,
`commit_verify.rs`, `managed_queue.rs`, `start_baseline.rs`) behind `POST /api/fleets/{fleet}/check`,
`/gate` and `/commit`, and by the run record. `run_id` is a soft reference to `launch.runs(run_id)`.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `run_id` | `TEXT NOT NULL` | The fleet run (soft reference) |
| `state` | `TEXT NOT NULL` | Hash of the tree state the check was asked for |
| `command` | `TEXT NOT NULL` | The check command |
| `started_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | When the row was opened |
| `finished_at` | `TIMESTAMPTZ NULL` | |
| `exit_code` | `INTEGER NULL` | |
| `output` | `TEXT NOT NULL DEFAULT ''` | The whole output |
| `askers` | `TEXT[] NOT NULL DEFAULT '{}'` | The seats that asked for this state |
| `told` | `TEXT[] NOT NULL DEFAULT '{}'` | The seats mike mailed because their file failed it |
| `state_after` | `TEXT NULL` | The tree state when the check finished; when it differs from `state` the tree changed during the run, so the result belongs to neither state and is not served to a later asker |
| `executing_at` | `TIMESTAMPTZ NULL` | When the managed command's process actually started (`launch/fleet/managed_command.rs`); NULL while queued |

**Indexes:**
- `idx_tree_checks_run_state`: `(run_id, state, id DESC)` (BTree)

---

### `launch.introductions`

Added by `20261001020000_launch_introductions.sql`. One row per slow command a fleet seat ran after
another seat of the same run had already run the same command: `again_use_id` is the repeat,
`first_use_id` the earlier run (both `tools.uses.id`, soft references). mike mails the coordinator
once per row so it can put the two seats in touch; the row is what keeps that to once. Written by
`launch/fleet/introduce.rs` (`fleet/record_introductions.sql`, `ON CONFLICT DO NOTHING`); a row
whose mail could not be queued is deleted again (`told_sql/release_introduction.sql`) so the
notice is retried, and `told_sql/recorded_introductions.sql` filters repeats already told.

| Column | Type | Purpose |
|---|---|---|
| `again_use_id` | `BIGINT` | Primary key; the repeat's `tools.uses.id` (soft reference) |
| `run_id` | `TEXT NOT NULL` | The fleet run (soft reference to `launch.runs`) |
| `first_use_id` | `BIGINT NOT NULL` | The earlier run's `tools.uses.id` (soft reference) |
| `told_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Indexes:**
- `idx_introductions_run`: `(run_id, told_at)` (BTree)

---

### `memory.seat_memory(p_project_id bigint)`

Added in `20261109130000_seat_memory_function.sql`. A read-only SQL function, not a table: the allow-listed
fleet-seat material of one project, for the search seat tier. Machine-opened operator turns are left out of
every operator-facing reader, so seat work reaches search only through this function. It returns one row per
included item, each labelled with `author` (`coordinator`, `seat`, `peer`, `subagent`), the run, seat and task, and
`report_filed` / `verdict_accepted` flags, and the reader must frame the text as data and not as something the
operator said. A function and not a view because every read is for one project: the seat set is narrowed to the
project first, and a view over all projects could not push a project filter below its window functions (about
530 ms per read against a 50 ms budget; the function takes about 47 ms for the largest project).

| Column | Type | Purpose |
|---|---|---|
| `item_kind` | `TEXT` | `seat_brief`, `workflow_brief`, `seat_report`, `seat_mail`, `peer_message`, `seat_goal_feedback` or `seat_thread_entry` |
| `project_id`, `session_id`, `session_uuid` | `BIGINT`, `BIGINT`, `UUID` | The project and the seat session the item belongs to |
| `run_id`, `seat`, `task` | `TEXT` | The fleet run, seat name and task; NULL for a `workflow_brief` |
| `author` | `TEXT` | Who wrote the text |
| `from_session`, `from_role` | `UUID`, `TEXT` | Mail kinds only: the sender's session and role (the channel tag's `from_role`, else the seat the sender was launched as); NULL elsewhere and when the sender is unknown |
| `at`, `text`, `source_ref` | `TIMESTAMPTZ`, `TEXT`, `TEXT` | When, the full text, and a stable reference (`launch:<id>`, `turn:<id>`, `note:<uuid>`, `mail:<uuid>`, `entry:<id>`) |
| `repeat_count` | `INTEGER` | How many near-identical rows collapsed into this one |
| `report_filed` | `BOOLEAN` | The item's own seat session filed a report |
| `verdict_accepted` | `BOOLEAN` | Every criterion's latest `verdict_recorded` for the (run, task) passed; false without a task or verdict |

Gates: the seat's run has a `report_filed` event, the launch did not fail, a template seat counts only when it
filed its own report, and observer seats and synthetic sessions are out. Empty turns, task notifications, loop
wakeups, tool-loaded, probes, compaction and observer mail are not memory and are left out. It reads
`launch.fleet_events` through `idx_fleet_events_session_kind` and `work.thread_log` through
`idx_work_thread_log_session_uuid`.

---




## Domain: `work`

Added by `20260903000002_work_threads.sql` ([issue #243](https://github.com/temet-ai/mike-private/issues/243),
moving worklog thread state out of `artifacts/threads/*/worklog.md` and the session-start hook that
pasted it into every session) and extended by `20261022120000_work_threads_direct_write.sql`
([issue #67](https://github.com/temet-ai/mike/issues/67): a thread lives in mike's database, not
in a file, so it can be created, checkpointed and given entries directly). Seven tables: `threads`
(identity and metadata), `thread_state` (the present tense, one row per thread, replaced whole on
every checkpoint), `thread_acceptance_criteria`, `thread_log` (append-only history),
`thread_sessions` (which sessions worked which thread), `thread_dependencies` and `thread_cards`
(the memory-keeper card rendered from the others). Not to be confused
with `mailbox.threads`, a conversation between sessions. Surfaced by `GET`/`POST /api/work-threads`
(`work_thread_list`, `work_thread_checkpoint`), `GET`/`PATCH /api/work-threads/{slug}`
(`work_thread_get`), `POST /api/work-threads/{slug}/entries` (`work_thread_add_entry`) and
`GET /api/work-threads/{slug}/export` (Markdown); persistence in `store/persistence/work_threads/`
(`import.rs`, `meta.rs`, `checkpoint.rs`, `lock.rs`, `reads.rs`, `criteria.rs`). Project knowledge
read and search (`project_knowledge/read.sql`, `search.sql`) include a project's threads.
Session references (`last_session_uuid`, `thread_log.session_uuid`, `thread_sessions.session_uuid`)
are deliberately soft, so a thread's history survives session rows being aged out.

### `work.threads`

One row per work thread: identity and metadata, parsed from a worklog by the import or created
directly. `source_path`/`parsed_at`/`parse_error` record ingestion honesty: a worklog that fails to
parse still gets a row rather than vanishing. `project_merge.rs` moves threads when projects merge.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `project_id` | `BIGINT NOT NULL` | FK → `projects.projects(id)` ON DELETE CASCADE |
| `slug` | `TEXT NOT NULL` | The thread's name in routes and tools; unique per project; CHECK not blank |
| `title` | `TEXT NULL` | |
| `status` | `TEXT NULL` | CHECK `IN ('active', 'implementing', 'review', 'parked', 'done', 'complete', 'in-progress')` |
| `branch` | `TEXT NULL` | |
| `worktree` | `TEXT NULL` | Path of the thread's git worktree, relative to the project root |
| `last_session_uuid` | `UUID NULL` | Soft reference to `sessions.sessions.session_uuid` |
| `source_path` | `TEXT NULL` | The worklog file an import read the thread from; NULL for a thread created in mike (NOT NULL dropped by `20261022120000`) |
| `parsed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `parse_error` | `TEXT NULL` | Non-NULL when the worklog was read but its fields could not be fully extracted |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `goal` | `TEXT NULL` | What the thread is for, in a sentence or two; not part of the replaced-whole state (`20261022120000`) |

**Constraints:**
- `threads_slug_not_blank`: `length(trim(slug)) > 0`
- `threads_status_check`: the status list above (dropped and re-added unchanged by `20261022120000`)
- `threads_has_title_or_error`: `title IS NOT NULL OR parse_error IS NOT NULL`
- `threads_project_slug_uniq`: UNIQUE `(project_id, slug)`

**Indexes:**
- `idx_work_threads_project_updated`: `(project_id, updated_at DESC)` (BTree) — a project's threads, newest first; also the `project_id` FK index

---

### `work.thread_state`

The present tense for one thread, REPLACED whole on every checkpoint (`work_threads/checkpoint.rs`)
and never appended to; the state it replaces moves to `work.thread_log` first as a
`state-replaced` entry. `thread_id` is the primary key, so one row per thread is a structural fact.
`tldr` is the injected unit and is length-bounded on purpose: the hook this replaces died of
unbounded accumulation. The embedding over the text columns is for similarity retrieval; the HNSW
index is partial, so a query must repeat `WHERE embedding IS NOT NULL` to use it.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT` | Primary key; FK → `work.threads(id)` ON DELETE CASCADE |
| `where_we_are` | `TEXT NOT NULL DEFAULT ''` | |
| `next_up` | `TEXT NOT NULL DEFAULT ''` | |
| `blockers` | `TEXT NOT NULL DEFAULT ''` | |
| `open_questions` | `TEXT NOT NULL DEFAULT ''` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Also the embedding staleness marker |
| `embedding` | `vector(768) NULL` | NULL until an embed worker computes it |
| `embedding_model` | `TEXT NULL` | |
| `embedded_at` | `TIMESTAMPTZ NULL` | |
| `tldr` | `TEXT NOT NULL DEFAULT ''` | The short summary a session reads to decide whether the thread is its business; CHECK `length(tldr) <= 600` |
| `source_updated_at` | `TIMESTAMPTZ NULL` | Worklog mtime when the embedding was taken, so unchanged text is not re-embedded |
| `decisions` | `TEXT NOT NULL DEFAULT ''` | Decisions in force, replaced whole like the other fields (`20261022120000`) |
| `files` | `TEXT[] NOT NULL DEFAULT '{}'` | Files the next session must read, relative to the project root (`20261022120000`) |

**Constraints:**
- `thread_state_embedding_consistent`: `embedding`, `embedded_at` and `embedding_model` all NULL or all non-NULL
- `thread_state_tldr_bounded`: `length(tldr) <= 600`

**Indexes:**
- `idx_work_thread_state_embedding_hnsw`: `embedding vector_cosine_ops WHERE embedding IS NOT NULL` (HNSW m=16 ef_construction=64, partial)

---

### `work.thread_acceptance_criteria`

The parsed `<acceptance-criteria>` block of a thread, one row per thread, with an embedding for
similarity retrieval. Same ingestion-honesty shape as `steering.thread_acceptance_criteria`, which
is a separate table still used by the false-done judge bridge (the migration header marks the two
as needing reconciliation). Written by `work_threads/criteria.rs` and the import.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT` | Primary key; FK → `work.threads(id)` ON DELETE CASCADE |
| `criteria_text` | `TEXT NULL` | |
| `source_path` | `TEXT NULL` | The worklog the criteria were read from; NULL for a thread created in mike (NOT NULL dropped by `20261022120000`) |
| `parsed_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Also the embedding staleness marker |
| `parse_error` | `TEXT NULL` | Non-NULL when the worklog was read but no acceptance-criteria block could be extracted |
| `embedding` | `vector(768) NULL` | |
| `embedding_model` | `TEXT NULL` | |
| `embedded_at` | `TIMESTAMPTZ NULL` | |

**Constraints:**
- `thread_acceptance_criteria_has_text_or_error`: `criteria_text IS NOT NULL OR parse_error IS NOT NULL`
- `thread_acceptance_criteria_embedding_consistent`: `embedding`, `embedded_at` and `embedding_model` all NULL or all non-NULL

**Indexes:**
- `idx_work_thread_acceptance_criteria_embedding_hnsw`: `embedding vector_cosine_ops WHERE embedding IS NOT NULL` (HNSW m=16 ef_construction=64, partial)

---

### `work.thread_log`

Append-only history for a thread: never edited or bulk-replaced. Sessions write `checkpoint`,
`decision`, `finding`, `brief`, `handoff` and `note` entries (`work_thread_add_entry`,
`work_threads/lock.rs`); a checkpoint writes a `state-replaced` entry holding the old state fields
in `data`. `ord` is the per-thread sequence, 1..n — a direct write takes `max(ord) + 1` while
holding the thread row lock, and an import uses the entry's position in the worklog's
"## Session log" section as its `ON CONFLICT` key.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY` | Primary key |
| `thread_id` | `BIGINT NOT NULL` | FK → `work.threads(id)` ON DELETE CASCADE |
| `ord` | `INTEGER NOT NULL` | 1-based position within the thread; CHECK `>= 1` |
| `entry_date` | `DATE NOT NULL DEFAULT current_date` | When the checkpoint happened (default added by `20261022120000`) |
| `session_uuid` | `UUID NULL` | Soft reference to `sessions.sessions.session_uuid`; NULL for an entry that names no session |
| `commit_sha` | `TEXT NULL` | Display metadata only, never indexed |
| `summary` | `TEXT NOT NULL` | The entry's first line; CHECK not blank |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | When the row was written, distinct from `entry_date` |
| `kind` | `TEXT NOT NULL DEFAULT 'checkpoint'` | CHECK `IN ('checkpoint', 'decision', 'finding', 'brief', 'handoff', 'note', 'state-replaced')` (`20261022120000`) |
| `body` | `TEXT NOT NULL DEFAULT ''` | The entry in full (`20261022120000`) |
| `data` | `JSONB NULL` | Structured payload: a `state-replaced` entry carries the old state fields; an imported worklog carries `{"imported_from": path}` (`20261022120000`) |

**Constraints:**
- `thread_log_ord_positive`: `ord >= 1`
- `thread_log_summary_not_blank`: `length(trim(summary)) > 0`
- `thread_log_thread_ord_uniq`: UNIQUE `(thread_id, ord)`
- `thread_log_kind_check`: the kind list above

**Indexes:**
- `idx_work_thread_log_thread_newest`: `(thread_id, entry_date DESC, ord DESC)` (BTree) — one thread's log, newest first
- `idx_work_thread_log_session_uuid`: `(session_uuid) WHERE session_uuid IS NOT NULL` (BTree, `20261109130002`) — the entries one session wrote, read by `memory.seat_memory`

---

### `work.thread_sessions`

The corpus edge: which sessions worked which thread, many-to-many. A new session is given a TLDR
plus these links rather than the accumulated text, and follows a link to that session's overview
when it needs the detail. Written by `work_threads/lock.rs` and `criteria.rs` on every write a
session makes to a thread, and by the import.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT NOT NULL` | FK → `work.threads(id)` ON DELETE CASCADE |
| `session_uuid` | `UUID NOT NULL` | Soft reference to `sessions.sessions.session_uuid` |
| `contribution` | `TEXT NOT NULL DEFAULT ''` | One line on what the session did for the thread; CHECK `length(contribution) <= 300` |
| `first_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |
| `last_seen_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- PRIMARY KEY `(thread_id, session_uuid)`
- `thread_sessions_contribution_bounded`: `length(contribution) <= 300`

**Indexes:**
- `idx_work_thread_sessions_recent`: `(thread_id, last_seen_at DESC)` (BTree) — which sessions worked this thread, most recent first
- `idx_work_thread_sessions_by_session`: `session_uuid` (BTree) — which threads did this session work

---

### `work.thread_dependencies`

Added by `20261022120000_work_threads_direct_write.sql`. Thread `thread_id` cannot finish before
thread `depends_on_thread_id`; both threads belong to the same project and the persistence layer
(`work_threads/meta.rs`) resolves the dependency by slug, so it survives a rename of the file that
used to name it.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT NOT NULL` | FK → `work.threads(id)` ON DELETE CASCADE |
| `depends_on_thread_id` | `BIGINT NOT NULL` | FK → `work.threads(id)` ON DELETE CASCADE |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | |

**Constraints:**
- PRIMARY KEY `(thread_id, depends_on_thread_id)`
- `thread_dependencies_no_self_reference`: `thread_id <> depends_on_thread_id`

**Indexes:**
- `idx_work_thread_dependencies_depends_on`: `depends_on_thread_id` (BTree)

---

### `work.thread_cards`

Added in `20261109120000_thread_cards.sql`. One rendered card per work thread: where it stands, what is
next, blockers, open questions, decisions in force, the newest handoff, decisions since it (log entries and
decision or finding notes tagged `thread:<slug>`), open dossier promises and unmet asks of the thread's
sessions from the last 14 days, each item with a reference token (`thread:`, `entry:`, `note:`,
`promise:`, `turn:`). Derived data, written only by the memory-keeper card worker
(`memory_keeper/worker.rs`, `store/persistence/work_threads/cards/`): the content upsert changes a row only
when `content_hash` differs, then a second statement advances `source_max_at`. Safe to delete; the next pass
rebuilds it.

| Column | Type | Purpose |
|---|---|---|
| `thread_id` | `BIGINT PRIMARY KEY` | FK → `work.threads(id)` ON DELETE CASCADE |
| `card` | `JSONB NOT NULL` | The card as structured sections, in text order |
| `text` | `TEXT NOT NULL` | The rendered card; CHECK `octet_length(text) <= 1500` |
| `content_hash` | `TEXT NOT NULL` | sha256 of `text`, hex; not indexed |
| `refs` | `JSONB NOT NULL` | Array of the reference tokens the text carries |
| `source_max_at` | `TIMESTAMPTZ NOT NULL` | Newest source change the card was built against; advanced on every rebuild |
| `built_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | When the content last changed |

**Constraints:**
- `thread_cards_text_bounded`: `octet_length(text) <= 1500`

**Triggers:** `trg_thread_cards_change_generation`/`_upd` bump the `thread_cards` row of
`memory.change_generations` when the content changes.

---

## Domain: `config`

Added in migrations `20260615000003_feature_toggles.sql` and `20260616000002_config_settings.sql`. Holds
global, non-session-scoped operator configuration — the runtime source of truth behind `/settings`.

### `config.feature_toggles`

One row per named feature toggle. Closed key space enforced by a CHECK constraint that every migration
introducing a new toggle drops and re-adds (idempotent `DROP CONSTRAINT IF EXISTS` + `ADD CONSTRAINT`).
`crates/mike/src/feature_toggles/mod.rs` is the authoritative list of toggle keys, and each
toggle's code default is documented in [`FEATURES.md`](FEATURES.md). Every toggle that rewrites
traffic or sends captured text off the machine defaults off. The column default is still `TRUE`, so a stored row is
what actually decides, and migration `20260917000002_fresh_install_defaults_off.sql` turns the
mutation and egress toggles off on a database that has captured no requests yet.

| Column | Type | Purpose |
|---|---|---|
| `key` | `TEXT PRIMARY KEY` | Toggle name; CHECK-constrained to the closed set |
| `enabled` | `BOOLEAN NOT NULL DEFAULT TRUE` | Whether the feature is currently active |
| `operator_set` | `BOOLEAN NOT NULL DEFAULT FALSE` | Whether an operator chose this value, as opposed to it being a migration seed |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Last time the toggle was written; drives the settings UI's "last changed" display |

**Indexes:** none beyond the PK — the table has ≤10 rows and is read at startup and on every settings-page load.

Migration `20260929000001_improvement_schema.sql` (JC21) adds `judge_lessons` to the closed key
set (the lesson proposer, `crates/mike/src/lesson_proposer/`). It sends captured turn text to an
LLM, so `agent-behavior-and-data.md` requires the code default off; the migration inserts
`(judge_lessons, false, false)` rather than any "on for this install" value a deploy records —
that flip is a deploy-time operator action through `/settings`, logged by the trigger below like
every other toggle write, never hardcoded into a migration.

---

### `config.settings`

Open-keyspace companion to `feature_toggles`: the expanded operator-tunable `ProxyConfig` surface exposed by
`/settings`. No CHECK constraint on `key` — the set of tunable settings grows over time and is validated
Rust-side against a compile-time `SETTINGS_META` registry instead of in the DB. No seed rows: an absent key
falls back to the compiled `ProxyConfig` default; writing a row overrides it; deleting the row restores the
default.

| Column | Type | Purpose |
|---|---|---|
| `key` | `TEXT PRIMARY KEY` | Dotted config path, e.g. `steerer.request_timeout_secs` |
| `value` | `JSONB NOT NULL` | JSON-encoded override (Bool/Int/Float/String/StringList/Enum kinds) |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Last time this setting was written |

**Indexes:** none beyond the PK — point lookups only, table stays small (~50 rows).

---

### `config.change_log`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `changed_at` | timestamptz NOT NULL DEFAULT now() |
| `xact_id` | xid8 NOT NULL DEFAULT pg_current_xact_id() |
| `subject_kind` | text NOT NULL CHECK (subject_kind IN ('setting', 'toggle', 'steerer', 'build', 'promotion')) |
| `subject_key` | text NOT NULL |
| `op` | text NOT NULL CHECK (op IN ('insert', 'update', 'delete', 'soft_delete', 'restore', 'observe')) |
| `old_value` | jsonb |
| `new_value` | jsonb |
| `redacted` | boolean NOT NULL DEFAULT false |
| `via` | text NOT NULL CHECK (via IN ('spa', 'api', 'startup', 'migration', 'promotion', 'mike', 'sql')) |
| `actor` | text |
| `request_id` | text |
| `build_sha` | text |
| `db_user` | text NOT NULL DEFAULT current_user |
| `application_name` | text |
| `detail` | jsonb NOT NULL DEFAULT '{}'::jsonb |

`subject_kind` has since been widened past this table's original set: `guard_threshold` and
`directive_decision` by `20260925000801_change_log_calibration.sql` (JC18), and `lesson_decision`
by `20260929000001_improvement_schema.sql` (JC21) — an approve or reject of a lesson candidate,
written by `store/persistence/lessons/proposals.rs` through `config.append_change` with
`subject_key` the lesson's `id` as text. `via` gained `mcp`, `cli` and `calibration` the same way;
the CHECK definitions above are as originally migrated, not the live constraint — read the
migration for the current set, per this file's own scope note.

## Evaluation metrics (`eval` schema, JC08)

Migration `20260924230000_eval_rates_alarms.sql`. Local SQL only; nothing here calls an LLM.

- **`eval.rollups`**: one committed daily rollup per `(for_day, sql_version)`; the UNIQUE constraint is the exactly-once claim (`INSERT ... ON CONFLICT DO NOTHING RETURNING id` opens the rollup transaction). `mode` is `scheduled`, `backfill` or `manual`; `labels_as_of` is the label instant the families read.
- **`eval.rates`**: append-only metric rows, each k of n over a stated `population`, window (`window_start`, `window_end`, `window_days`), `role`, bundle (`bundle_id`, or `bundle_label` = `legacy:<model>:<rubric>` for verdicts written before bundles), `axis`/`class`, label slice (`label_tier`, `label_split`, `label_source`, never pooled), optional `dimension`/`dimension_value` and steerer fields, with `value`, the Wilson interval (`ci_low`, `ci_high`), `weighted`/`n_effective` for Horvitz-Thompson weighted slices, and `detail`. Families: `accuracy` (precision and recall per `eval.class_map` row), `label_mix` (stored label shares, current 7 days against the 28 before, plus the corrected share on replied real turns), `base_rate` (30 and 90 days, dev labels and production), `steer_calibration` (fire rate from `steering.evaluations`, fire precision from intervention labels, the verifier baseline against the census). No foreign keys on `bundle_id`, `project_id`, `steerer_id`.

`action_effect` (JC21, "judgement at work") is a fifth family, added by migration `20260929000001_improvement_schema.sql` widening both the bare `family` CHECK and the `family`+`metric` pairing CHECK (`eval_rates_family_metric`, itself widened by `20260925000801_change_log_calibration.sql`) with `(family = 'action_effect' AND metric = 'share')`. One row per `(action, population)`: `dimension = 'lesson'` with `dimension_value` the lesson id (text) and `population` `before`/`after` split at the lesson's promotion-to-daily moment (`config.change_log` where `subject_kind = 'lesson_decision'` and `new_value ->> 'stage' = 'daily'`, or the lesson's own `created_at` for a directive-sourced lesson that starts at stage `daily`), each side a `EFFECT_WINDOW_DAYS` (14) window; or `dimension = 'guard_question'` with `dimension_value` the question code and `population` `holdback`/`treated`, keyed off `steering.evaluations.arm`. Written by `PersistHandle::write_action_effect_rows` (`store/persistence/improvement/effect.rs`), called by `crates/mike/src/config/subcommands/eval/rollup_improvement.rs` against an `eval.rollups` row claimed under its own `sql_version` (`action-effect-v1`, `current_or_claimed_action_effect_rollup_id`) so it never collides with the daily family rollup. The read model (`store/persistence/improvement/actions.rs`, `interval.rs`) picks the latest row per `(dimension_value, population)` and reports `not_enough_data` below `READ_POINT_N` (20) per side, else a Newcombe interval for the population-pair difference — each side's own Wilson bound, combined per Newcombe (1998); `interval.rs` unit-tests the arithmetic against hand-verified cases with no DB.
- **`eval.alarms`**: episodes, one open per `(kind, subject)` (partial unique index `uq_eval_alarms_open`), extended while firing and cleared after three quiet evaluations; a later firing opens a new row. Kinds: `label_mix_shift`, `unjudged_backlog`, `judge_stalled`, `budget_refusals`, `change_without_gate`, `gold_label_by_agent`. `acknowledged_at` and `ack_note` are the operator's and do not clear the episode.
- **`eval.class_map`**: which production class is scored against which label value (three legacy rows; `20260924230100_class_map_two_axis.sql` adds the eleven reaction and delivery rows).
- **`eval.turn_population_v`**: closed turns with `is_real` (JC02 operator session and genuine prompt, no agent-id start request, not in `eval.population_exclusions`), `is_replied`, `next_state`, `ended_on`, `close_cause`, the outcome's `bundle_id`/`bundle_label` and `writer_role`.
- **`eval.wilson_low(p, n)`, `eval.wilson_high(p, n)`**: the Wilson 95% interval every family uses; NULL when n is 0.

## Evaluation runs and gates (`eval` schema, JC10)

Migrations `20260925000000_llm_calls_role_eval.sql` (the `llm.calls.role` CHECK gains `eval`) and `20260925000100_eval_runs.sql`.

- **`eval.runs`**: one row per `mike eval run`: a fixed label set (`split`, `tiers`, `holdout_batch`, `labels_as_of`, `label_count`, `label_ids_hash`, compared in code and never indexed) judged by one configuration (`bundle_id`, `rubric_version`, `input_builder_version`, `slices`, `model`). Mutable only in `status` (`running`, `paused_budget`, `complete`, `failed`, `cancelled`), `finished_at`, `cost_usd`, the item counters and `report`. `argv` holds the label options a resume reloads.
- **`eval.run_items`**: one row per subject turn: the gold `reaction_label_id`/`delivery_label_id`, the `memory.turn_verdicts` row (`mode = 'eval'`) holding the verdict and its whole input, `outcome` (`judged`, `refused`, `failed`, `skipped_unresolved`), `stratum`, `inclusion_prob` (nullable, never defaulted) and the correctness flags.
- **`eval.gates`**: one row per `mike eval gate` decision (candidate, baseline, optional noise run, `holdout_batch`, `verdict`, `floors`, `report`, `override_reason`), kept whatever the verdict so holdout reuse is countable. `eval.bundles.gate_run_id` now references it, and the bundle role CHECK gains `turn_judge_shadow`.

### `eval.alarms`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `kind` | text NOT NULL CHECK (kind IN ('label_mix_shift', 'unjudged_backlog', 'judge_stalled', 'budget_refusals', 'change_without_gate', 'gold_label_by_agent')) |
| `subject` | text NOT NULL |
| `first_seen_at` | timestamptz NOT NULL DEFAULT now() |
| `last_seen_at` | timestamptz NOT NULL DEFAULT now() |
| `cleared_at` | timestamptz |
| `evaluations` | integer NOT NULL DEFAULT 1 CHECK (evaluations >= 1) |
| `quiet_evaluations` | integer NOT NULL DEFAULT 0 CHECK (quiet_evaluations >= 0) |
| `detail` | jsonb NOT NULL |
| `acknowledged_at` | timestamptz |
| `ack_note` | text |

### `eval.axis_labels`

| Column | Definition |
|---|---|
| `axis` | text NOT NULL |
| `label` | text NOT NULL |
| `subject_kinds` | text[] NOT NULL |
| `sort_order` | integer NOT NULL |
| `description` | text NOT NULL |

### `eval.bundles`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `role` | text NOT NULL CHECK (role IN ('turn_judge', 'session_judge', 'steerer')) |
| `subject_key` | text NOT NULL DEFAULT '' |
| `origin` | text NOT NULL CHECK (origin IN ('resolver', 'candidate')) |
| `status` | text NOT NULL CHECK (status IN ('candidate', 'shadow', 'live', 'live_ungated', 'retired')) |
| `components` | jsonb NOT NULL |
| `content_sha256` | text NOT NULL |
| `build_sha` | text NOT NULL |
| `build_dirty` | boolean NOT NULL DEFAULT false |
| `cause` | jsonb NOT NULL DEFAULT '{}'::jsonb |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `superseded_at` | timestamptz |
| `superseded_by_id` | bigint REFERENCES eval.bundles(id) |
| `gate_run_id` | bigint |
| `promoted_at` | timestamptz |
| `override_reason` | text |

### `eval.class_map`

| Column | Definition |
|---|---|
| `production_axis` | text NOT NULL |
| `production_class` | text NOT NULL |
| `label_axis` | text NOT NULL |
| `label_value` | text NOT NULL |
| `replied_counts_as_negative` | boolean NOT NULL DEFAULT false |

### `eval.embedding_drift`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `day` | date NOT NULL |
| `stream` | text NOT NULL CHECK (stream IN ('prompt', 'ending', 'correction')) |
| `embedder_key` | text NOT NULL |
| `drift_version` | text NOT NULL |
| `n_current` | integer NOT NULL CHECK (n_current >= 0) |
| `n_baseline` | integer NOT NULL CHECK (n_baseline >= 0) |
| `distance` | double precision |
| `z` | double precision |
| `prior_days` | integer NOT NULL CHECK (prior_days >= 0) |
| `prior_mean` | double precision |
| `prior_sd` | double precision |
| `status` | text NOT NULL CHECK (status IN ('scored', 'warming', 'too_few_items')) |
| `computed_at` | timestamptz NOT NULL DEFAULT now() |

### `eval.gates`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `candidate_run_id` | bigint NOT NULL REFERENCES eval.runs(id) |
| `baseline_run_id` | bigint NOT NULL REFERENCES eval.runs(id) |
| `noise_run_id` | bigint REFERENCES eval.runs(id) |
| `holdout_batch` | text |
| `verdict` | text NOT NULL CHECK (verdict IN ('pass', 'fail', 'incomplete')) |
| `floors` | jsonb NOT NULL |
| `report` | jsonb NOT NULL |
| `override_reason` | text |

### `eval.import_batches`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `source_key` | text NOT NULL UNIQUE |
| `manifest` | jsonb NOT NULL |
| `counts` | jsonb NOT NULL |
| `imported_at` | timestamptz NOT NULL DEFAULT now() |
| `imported_by` | text NOT NULL |

### `eval.labels`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `subject_kind` | text NOT NULL CHECK (subject_kind IN ('turn', 'session_end', 'evaluation', 'intervention')) |
| `subject_id` | bigint NOT NULL |
| `operator_turn_id` | bigint NOT NULL |
| `session_id` | bigint NOT NULL |
| `session_uuid` | uuid NOT NULL |
| `axis` | text NOT NULL |
| `label` | text NOT NULL |
| `action` | text NOT NULL DEFAULT 'assert' CHECK (action IN ('assert', 'withdraw')) |
| `tier` | text NOT NULL CHECK (tier IN ('gold', 'relayed', 'auto_shadow', 'seed')) |
| `method` | text NOT NULL CHECK (method IN ('operator_click', 'operator_confirmed_relay', 'relayed_quote', 'next_message_auto', 'double_blind_adjudicated', 'single_model')) |
| `split` | text NOT NULL |
| `tags` | text[] NOT NULL DEFAULT '{}' |
| `severity` | text CHECK (severity IN ('none', 'minor', 'major', 'severe')) |
| `confidence` | text |
| `evidence` | text |
| `note` | text |
| `source` | text NOT NULL |
| `provenance` | jsonb NOT NULL DEFAULT '{}'::jsonb |
| `inclusion_prob` | double precision CHECK (inclusion_prob > 0 AND inclusion_prob <= 1) |
| `verdict_snapshot` | jsonb |
| `verdict_bundle_id` | bigint |
| `import_batch_id` | bigint REFERENCES eval.import_batches(id) |
| `supersedes_id` | bigint REFERENCES eval.labels(id) |
| `created_at` | timestamptz NOT NULL DEFAULT now() |

An eval-run review (the `/sessions/labelling/accuracy/{run}/review` page, Sessions › Review labels) is stored as ordinary gold
labels, not a separate table. Recording a decision posts the axes the operator answered (the
disputed ones; an undisputed axis is not re-saved) through the SPA-only
`POST /api/eval/labels` with `context = 'eval_review'` and `eval_run_id`, which lands in
`provenance.eval_run_id`; the handler refuses an `eval_run_id` whose run has no item for the
turn. A decision that keeps the run's gold writes the same labels at tier `gold`, which is a
true statement (the operator confirmed them) and changes no future label set's values.
"Reviewed for run N" means a gold assert with `provenance ->> 'eval_run_id' = N` exists.
`eval.run_items`, the seed rows it points at and the run's report are never rewritten, and
`eval.labels_as_of(run.labels_as_of)` still reproduces the labels the run was scored on.

### `eval.population_exclusions`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `session_id` | bigint NOT NULL |
| `operator_turn_id` | bigint |
| `reason` | text NOT NULL CHECK (reason IN ('skill_eval_batch', 'scratch_project', 'probe_prompt')) |
| `import_batch_id` | bigint NOT NULL REFERENCES eval.import_batches(id) |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `withdrawn_at` | timestamptz |

### `eval.rates`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `rollup_id` | bigint NOT NULL REFERENCES eval.rollups(id) ON DELETE CASCADE |
| `family` | text NOT NULL CHECK (family IN ('accuracy', 'label_mix', 'base_rate', 'steer_calibration', 'pattern_tagging')) |
| `metric` | text NOT NULL CHECK (metric IN ('precision', 'recall', 'share', 'fire_rate', 'fire_precision', 'fire_recall')) |
| `population` | text NOT NULL CHECK (population IN ('real_turns', 'replied_real_turns', 'real_session_ends', 'steerer_evaluations', 'labelled_turns')) |
| `scope_kind` | text NOT NULL CHECK (scope_kind IN ('global', 'project')) |
| `project_id` | bigint |
| `window_start` | timestamptz |
| `window_end` | timestamptz NOT NULL |
| `window_days` | integer CHECK (window_days IS NULL OR window_days > 0) |
| `role` | text NOT NULL CHECK (role IN ('turn_judge', 'session_judge', 'steerer')) |
| `bundle_id` | bigint |
| `bundle_label` | text |
| `axis` | text NOT NULL |
| `class` | text NOT NULL |
| `label_tier` | text |
| `label_split` | text |
| `label_source` | text |
| `dimension` | text |
| `dimension_value` | text |
| `steerer_id` | bigint |
| `question` | text |
| `mode` | text |
| `k` | integer NOT NULL CHECK (k >= 0) |
| `n` | integer NOT NULL CHECK (n >= k) |
| `value` | double precision |
| `ci_low` | double precision |
| `ci_high` | double precision |
| `weighted` | boolean NOT NULL DEFAULT false |
| `n_effective` | double precision |
| `detail` | jsonb NOT NULL DEFAULT '{}'::jsonb |

### `eval.rollups`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `for_day` | date NOT NULL |
| `sql_version` | text NOT NULL |
| `mode` | text NOT NULL CHECK (mode IN ('scheduled', 'backfill', 'manual')) |
| `labels_as_of` | timestamptz NOT NULL |
| `started_at` | timestamptz NOT NULL DEFAULT now() |
| `finished_at` | timestamptz |
| `rows_written` | integer |

### `eval.run_items`

| Column | Definition |
|---|---|
| `run_id` | bigint NOT NULL REFERENCES eval.runs(id) ON DELETE CASCADE |
| `operator_turn_id` | bigint NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE |
| `reaction_label_id` | bigint REFERENCES eval.labels(id) |
| `delivery_label_id` | bigint REFERENCES eval.labels(id) |
| `turn_verdict_id` | bigint REFERENCES memory.turn_verdicts(id) ON DELETE SET NULL |
| `outcome` | text NOT NULL CHECK (outcome IN ('judged', 'refused', 'failed', 'skipped_unresolved')) |
| `stratum` | text NOT NULL DEFAULT 'all' |
| `inclusion_prob` | double precision CHECK (inclusion_prob IS NULL OR (inclusion_prob > 0 AND inclusion_prob <= 1)) |
| `reaction_correct` | boolean |
| `delivery_correct` | boolean |
| `created_at` | timestamptz NOT NULL DEFAULT now() |

### `eval.runs`

| Column | Definition |
|---|---|
| `id` | bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY |
| `created_at` | timestamptz NOT NULL DEFAULT now() |
| `finished_at` | timestamptz |
| `purpose` | text NOT NULL CHECK (purpose IN ('measure', 'noise', 'ablation')) |
| `status` | text NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'paused_budget', 'complete', 'failed', 'cancelled')) |
| `bundle_id` | bigint NOT NULL REFERENCES eval.bundles(id) |
| `rubric_version` | text NOT NULL |
| `input_builder_version` | text NOT NULL |
| `slices` | text[] NOT NULL DEFAULT '{}' |
| `model` | text NOT NULL |
| `split` | text NOT NULL CHECK (split IN ('dev', 'holdout')) |
| `tiers` | text[] NOT NULL |
| `holdout_batch` | text |
| `labels_as_of` | timestamptz NOT NULL |
| `label_count` | integer NOT NULL CHECK (label_count >= 0) |
| `label_ids_hash` | text NOT NULL |
| `repeat_of_run_id` | bigint REFERENCES eval.runs(id) |
| `max_usd` | double precision NOT NULL CHECK (max_usd >= 0) |
| `cost_usd` | double precision NOT NULL DEFAULT 0 |
| `items_judged` | integer NOT NULL DEFAULT 0 |
| `items_refused` | integer NOT NULL DEFAULT 0 |
| `items_failed` | integer NOT NULL DEFAULT 0 |
| `report` | jsonb |
| `argv` | jsonb NOT NULL |

### `eval.turn_splits`

| Column | Definition |
|---|---|
| `operator_turn_id` | bigint PRIMARY KEY |
| `split` | text NOT NULL CHECK (split IN ('dev', 'holdout')) |
| `split_reason` | text NOT NULL CHECK (split_reason IN ('seed_import', 'holdout_import', 'hash')) |
| `assigned_at` | timestamptz NOT NULL DEFAULT now() |

## Domain: `sync`

Added in migration `20260621000001_sync_bookkeeping.sql`. Source-side bookkeeping for the local-DB-fallback
resync worker (see [`LOCAL_DB_FALLBACK.md`](LOCAL_DB_FALLBACK.md)) — durable state the worker reads and writes
when pushing locally-captured rows up to the remote DB. Only populated on a local fallback DB; empty and
harmless on the remote DB or any instance that never acted as a fallback source (every migration runs on
every DB).

### `sync.cursor`

Per-source-table high-water mark of the largest local surrogate id already pushed to the remote DB. Fact
tables are append-only (`GENERATED ALWAYS AS IDENTITY` ids only grow), so `id > last_local_id` is exactly the
unsynced set — an incremental, resumable, idempotent cursor.

| Column | Type | Purpose |
|---|---|---|
| `table_name` | `TEXT PRIMARY KEY` | Schema-qualified source table, e.g. `requests.requests` |
| `last_local_id` | `BIGINT NOT NULL DEFAULT 0` | Largest local id of this table already pushed remotely |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | Last cursor advance |

**Indexes:** none beyond the PK — one row per synced fact table, never joined on.

---

### `sync.id_map`

Durable local→remote surrogate id remapping. Local and remote DBs assign `GENERATED ALWAYS AS IDENTITY` ids
independently, so a local row's id is meaningless remotely; the worker records the mapping here after each
remote insert and resolves every child FK through it before inserting the child. Persisting it (rather than
holding it in memory per run) is what makes replay idempotent across restarts — a `(table_name, local_id)`
already present is already remote and is skipped, not re-inserted.

| Column | Type | Purpose |
|---|---|---|
| `table_name` | `TEXT NOT NULL` | Schema-qualified source table; part of the composite PK |
| `local_id` | `BIGINT NOT NULL` | Surrogate id in the LOCAL DB; part of the composite PK |
| `remote_id` | `BIGINT NOT NULL` | Surrogate id the same row received in the REMOTE DB |
| `synced_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | When the mapping was recorded |

**Constraints:** `PRIMARY KEY (table_name, local_id)` — also serves as the worker's "already synced?" lookup index.

**Indexes:** none beyond the PK.

---

## Domain: `jobs`

### `jobs.runs`

Added in migration `20261031120000_scheduled_job_runs.sql`. One row per finished run of a
scheduled job (`crates/mike/src/scheduled_jobs/jobs.rs` `JOBS`), written by the job's worker when
the run ends (`PersistHandle::record_scheduled_job_run`). When the proxy starts, each worker reads
its latest row and its latest `schedule` row (`PersistHandle::latest_scheduled_job_runs`) to
restore the home page's Scheduled jobs panel, the period slot that already ran, and the failure
backoff. A run cut short by a proxy shutdown writes no row, so its slot runs again after the
restart. About fifteen rows a day per machine; no retention sweep.

| Column | Type | Purpose |
|---|---|---|
| `id` | `BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | Surrogate key |
| `job_name` | `TEXT NOT NULL` | `Job.name`: `learning_promote`, `learning_audit`, `compressor_loop`, `cargo_sweep`, `lessons_commit`, `usage_audit` |
| `machine_uuid` | `UUID NULL` | The machine that ran it (`machine.json`); `NULL` when that proxy had no identity. Per-machine jobs restore only their own machine's rows; leased jobs restore from any machine |
| `trigger` | `TEXT NOT NULL` | `schedule` or `manual` (`POST /api/scheduled-jobs/{name}/run`). Only `schedule` rows restore the period slot |
| `started_at` / `finished_at` | `TIMESTAMPTZ NOT NULL` | When the run started and ended; `finished_at >= started_at` |
| `duration_ms` | `BIGINT NOT NULL` | Wall time of the script |
| `result_kind` | `TEXT NOT NULL` | `succeeded`, `alarmed`, `exited`, `signalled`, `timed_out`, `unrunnable` (`runner::RunResult`) |
| `exit_code` | `INTEGER NULL` | Set exactly when `result_kind` is `succeeded`, `alarmed` or `exited` (CHECK) |
| `result_detail` | `TEXT NULL` | Why an `unrunnable` run could not start |
| `output_tail` | `TEXT NOT NULL` | The last 20 lines of stdout and stderr the runner keeps, newline-joined |
| `consecutive_failures` | `INTEGER NOT NULL` | Failures in a row after this run was recorded |
| `retry_after` | `TIMESTAMPTZ NULL` | The schedule's backoff hold after this run; `NULL` when none |

**Indexes:** `idx_jobs_runs_job_started`: `(job_name, started_at DESC)` — the latest-run reads.

---

## Read-API Views (`mike` schema)

The views below project domain tables for the Rust REST/MCP layer. Use the current `mike`
schema name when querying them; `claude_proxy` was renamed in July 2026.

### `mike.file_touches_v`

Thin pass-through over the capture-time `file_path` and `operation` columns on `tools.uses`. No CASE, no
UNION ALL, no tool-name list — path resolution happened at capture time in the Rust registry.

```sql
SELECT wire_tool_use_id, request_id, file_path, operation
FROM tools.uses
WHERE file_path IS NOT NULL
```

### `mike.operator_turn_aggregates_v`

Per-turn counts, sums, and arrays. Closed turns read the materialized cache columns on
`memory.operator_turns`. The single open turn computes live via two `GROUP BY` subqueries (joined through
`r.operator_turn_id = ot.id`). Token sums are in a separate subquery to avoid the
uses×diagnostics cross-product inflating them.

Columns: `turn_id`, `tool_call_count`, `tool_error_count`, `weasel_findings_count`, `total_input_tokens`,
`total_output_tokens`, `tools_used`, `agents_dispatched`, `files_touched`.

### `mike.operator_turn_text_v`

User prompt and concatenated assistant text per turn. Closed turns read
`memory.operator_turns.final_assistant_text`. The open turn `string_agg`s live from
`responses.stream_chunks` joined via `r.operator_turn_id = ot.id`. Columns: `turn_id`, `user_prompt`,
`final_assistant_text`.

### `mike.session_quality_v`

Added in migration `20260625000002_session_judge.sql` (GitHub issue temet-ai/mike-private#24). One row per session that has at
least one `session_judges.verdicts` row, carrying its **latest** verdict via `DISTINCT ON (v.session_id)
ORDER BY v.session_id, v.judged_at DESC, v.id DESC` — set-based, no correlated scalar subquery. Sessions
with no verdict row are excluded (`session_summary_v` LEFT JOINs this view to fill that gap with NULLs).

Columns: `session_id` (= `session_uuid::text`), `outcome`, `quality_score`, `rationale`, `judge_model`,
`rubric_version`, `judged_at`, `bundle_id`, then `verdict_id`, `end_turn_id`, `end_label`, `end_source`,
`end_rule`, `end_confidence`, `end_evidence`, `failure`, `failure_tags`, `severity`. It reads `mode = 'live'`
rows only.

### `eval.session_end_run_items`

Added in migration `20260925000500_session_end_classification.sql`. One row per ending turn of a
`mike eval run --role session_judge`: the gold `label` it was scored against, the `session_judges.verdicts`
row (`mode = 'eval'`) holding the whole input and output, the model's own `llm_end_label`, the
deterministic rule whose conditions held (`rule_fired`, recorded whether or not the run applied it), the
`effective_end_label` and the two correctness flags. `mike eval gate --role session_judge` scores four
variants (model only, +fleet-seat-v1, +done-v1, both) from it. Read at
`GET /api/eval/runs/{id}/session-end-items` (MCP `list_eval_session_end_items`).

Columns added by the judgement-context migrations (definition as migrated):

| Column | Definition |
|---|---|
| `run_id` | bigint NOT NULL REFERENCES eval.runs(id) ON DELETE CASCADE |
| `end_turn_id` | bigint NOT NULL REFERENCES memory.operator_turns(id) ON DELETE CASCADE |
| `session_id` | bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE |
| `label_id` | bigint NOT NULL REFERENCES eval.labels(id) |
| `verdict_id` | bigint NULL REFERENCES session_judges.verdicts(id) ON DELETE SET NULL |
| `outcome` | text NOT NULL CHECK (outcome IN ('judged', 'refused', 'failed', 'skipped_unresolved')) |
| `label` | text NOT NULL CHECK (label IN ('done', 'closed_in_frustration', 'cut_off_neutral', 'fleet_seat', 'unclear')) |
| `llm_end_label` | text NULL CHECK (llm_end_label IS NULL OR llm_end_label IN ('done', 'closed_in_frustration', 'cut_off_neutral', 'fleet_seat', 'unclear')) |
| `rule_fired` | text NULL CHECK (rule_fired IS NULL OR rule_fired IN ('fleet-seat-v1', 'done-v1')) |
| `effective_end_label` | text NULL |
| `llm_frustration_correct` | boolean NULL |
| `effective_correct` | boolean NULL |
| `created_at` | timestamptz NOT NULL DEFAULT now() |

### `mike.session_summary_v`

One row per session with project-joined metadata. Single-pass `GROUP BY (s.id, p.id, mdl.last_models,
sq.outcome, sq.quality_score, sq.judged_at)` — no correlated subqueries. `request_count` and `last_models`
are computed here (not stored on `sessions.sessions`). `last_path` was dropped (always `/v1/messages`, no
information content). `session_outcome`/`session_quality_score`/`session_judged_at` are sourced via LEFT
JOIN to `session_quality_v` and NULL when the session has not yet been judged. `overview`/
`overview_generated_at` pass through unchanged from `sessions.sessions` (see the `sessions` domain above for
the tri-state invariant).

Columns: `session_id` (= `session_uuid::text`), `project_path`, `first_seen`, `last_seen`, `last_status`,
`title`, `pinned`, `project_pinned`, `request_count`, `last_models`, `session_outcome`,
`session_quality_score`, `session_judged_at`, `overview`, `overview_generated_at`.

---

## Triggers

### NOTIFY triggers (power the WS live feed — channel `proxy.row_changed`)

| Trigger | Table | Timing | Channel |
|---|---|---|---|
| `trg_requests_row_changed` | `requests.requests` | AFTER INSERT OR UPDATE OR DELETE | `proxy.row_changed` |
| `trg_sessions_row_changed` | `sessions.sessions` | AFTER UPDATE | `proxy.row_changed` |
| `trg_filter_rules_row_changed` | `rules.rules` | AFTER INSERT OR UPDATE OR DELETE | `proxy.row_changed` |
| `trg_prompt_compositions_row_changed` | `overrides.overrides` | AFTER INSERT OR UPDATE OR DELETE | `proxy.row_changed` |

There is no `trg_lessons_row_changed` in the current schema.

### Fill triggers (retired in the migration squash)

The 10 fill triggers and 6 fill functions that previously back-filled `*_ref` text/UUID columns from newly
inserted bigint FKs were dropped in the clean-break squash. The Rust write path now sets bigint FK columns
directly. None of the following exist in the current schema:

`trg_agent_dispatches_fill_refs`, `trg_agent_instances_fill_session_ref`,
`trg_operator_turns_fill_session_ref`, `trg_prompt_composition_revisions_fill_composition_ref`,
`trg_prompt_compositions_fill_system_prompt_ref`, `trg_request_deltas_fill_session_ref`,
`trg_request_messages_fill_message_ref`, `trg_requests_fill_session_ref`,
`trg_response_stream_chunks_fill_session_ref`, `trg_tool_results_fill_tool_use_ref`.

---

## FK Cycle and Trailing ALTER TABLE Block

Three FKs cannot be declared inline because of schema dependency ordering or a true cycle:

1. **`requests.requests.agent_instance_id → agents.instances(id)` ON DELETE SET NULL** — part of the
   true FK cycle: `requests → agent_instances → agent_dispatches → tool_uses → requests`. No table-creation
   order resolves it.

2. **`requests.system_prompts.override_id → overrides.overrides(id)`** — forward edge: the `requests`
   schema block is created before the `overrides` block.

3. **`requests.requests.operator_turn_id → memory.operator_turns(id)` ON DELETE SET NULL** — forward edge:
   the `requests` block is created before the `memory` block. `operator_turn_id` is the canonical
   turn-membership fact; the turn-classifier stamps it after the turn row exists.

All three constraints are fully enforced at runtime. The deferral is a DDL-ordering device only.

## Semantic context (JC20)

Migration `20260925000400_semantic_context.sql`.

- `memory.semantic_embedders`: one row per embedding space: `embedder_key` (unique, plain text), model, dim, chunk size, prefix version, `urls_seen`, `canary_version`, `canary_vectors` (the eight canary vectors whole), `status` (`current` or `retired`, one current), `first_seen_at`, `last_checked_at`, `copy_valid_from` and `copy_verify` (set together by `mike semantic embedders verify`).
- `memory.semantic_items`: one embedded text per row: `kind` (`turn_prompt`, `turn_ending`, `directive`, `anchor`), session and turn (cascading, NULL for anchors), project, request, `origin_session_uuid` and `origin_turn_id` (anchors), `source_ref`, `pattern` (anchors), `text` (exactly what was embedded, whole), `embedding vector(768)`, `embedder_key`, `vector_source` (`chunk_copy`, `embedded`, `member_copy`), `builder_version`, `effective_at`, `recorded_at`, `data`. Unique on `(kind, source_ref, embedder_key, builder_version)`; HNSW on `embedding`.
- `memory.semantic_floors`: append-only chance floors per `(embedder_key, pair, quantile)` with p50, p95, p99, p99.9, `n_pairs`, the sample, and `status` (`in_force`, `monitoring`, `insufficient`).
- `memory.pattern_candidates`: groups of untagged corrections with every member id, a medoid, `status` (`open`, `named`, `dismissed`, `superseded`) and the operator's decision.
- `memory.pattern_assignments`: the tagger's output per qualified correction: `pattern` (NULL = unassigned), score, similarity, top three anchors, runner-up, margin, `qualification` and its reference, floor id and versions.
- `eval.embedding_drift`: one row per day, stream (`prompt`, `ending`, `correction`), embedder key and drift version: counts, distance, z, prior statistics and `status` (`scored`, `warming`, `too_few_items`).
- `idx_turn_facts_next_genuine_turn` on `memory.turn_facts (next_genuine_turn_id)`.

The migration adds `embedding_drift` to `eval.alarms.kind` and `semantic_context` (on) to `config.feature_toggles`.
