# Migration Squash — Planned vs Done (Per-Table, Per-Column)

For each table: the **Planned** target (from [`migration-squash-spec.md`](migration-squash-spec.md), 2026-06-04) beside what was **Done**, then a per-column table that answers, for every column: is it the **PK / identity**, does it carry a **UNIQUE / FK**, is it **indexed**, and its **status**. Sourced directly from `20260604000002_tables.sql` (columns, PK, uniques, checks), `…03_constraints.sql` (every FK), `…04_indexes_functions_triggers.sql` (every index).

> **State:** reflects the **working tree** of `feature/migration-squash` — the redundant-`UNIQUE(id)` drop (×7) and the 3 soft-pointer FKs are **applied but uncommitted**. Fresh-apply + `sqlx prepare` + full-suite verification (task #3) is still outstanding; this doc does not claim the suite is green.

**Totals** — Planned: **25** tables. Done: **24** (`backfill_messages_progress` dropped). **45 FKs** · 0 `*_ref` columns · every table `id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY` (now the single unique on `id`).

**Status legend:** ✓ correct / done · ⚠ open decision (not yet applied) · ⓘ by-design / external (no change).

## ✓ Resolved (working tree) — redundant `UNIQUE (id)` dropped on all 7 tables

Each of these declared `id … PRIMARY KEY` **and** a leftover `uq_<table>_id UNIQUE (id)`. The PK already enforces uniqueness; the second constraint was redundant cruft. `messages` was cleaned earlier (`ed5d4d2`); the other 7 — **sessions, agents, filter_rules, system_prompts, prompt_compositions, tool_uses, lessons** — are now dropped in `tables.sql` (uncommitted). `id` remains UNIQUE+NOT NULL via the PK alone.

## ✓ Resolved (working tree) — 3 soft-pointer FKs added

`agent_instances.first_request_id`, `request_deltas.predecessor_request_id`, `lessons.request_id` now carry `FK → requests(id) ON DELETE SET NULL` (all nullable). At committed HEAD these had no FK — the earlier reaudit was correct for HEAD; the FKs were added as part of this fix.

## ⚠ Outstanding design decisions (from the per-column review)

| # | Item | Decision needed |
|---|---|---|
| 2 | `request_messages.message_sha256` / `request_deltas.message_sha256` denormalized — derivable via the `message_id` FK | drop (normalize) vs keep (direct content lookup; `idx_request_messages_sha` reads it today). Code change in `messages.rs`. |
| 4 | `request_system_prompts.firing_rule_ids uuid[]` — array of `filter_rules.uuid`, not id-based, no FK possible on array elements | `bigint[]` (id-consistent, still no FK) vs junction table `request_system_prompt_firing_rules` (real FK). |
| 6 | `sessions.session_id text` — values are uuids but typed text (locked-rule-3) | tighten to `uuid`? High blast radius (`String` across proxy/API/frontend). |

## ⓘ By-design (flagged in review, confirmed correct in source)

| Item | Why it's correct |
|---|---|
| `request_responses.upstream_request_id` / `upstream_message_id` text, no FK | External **Anthropic** ids — upstream `request-id` header (`handler.rs:945`) and `msg_…` from SSE `message_start` (`parser.rs:184`). Not local rows. |
| `tool_uses.tool_id` text, no FK | External Anthropic `toolu_…` id; there is no `tools` table (`tool_uses` *is* it). Natural key. |
| `system_prompts` has no `session_id`/`request_id` | Content-addressed dedup (one row per unique prompt, shared by many requests). The link lives on `request_system_prompts (request_id → system_prompt_id)`. |

## Cross-cutting (spec open questions)

| Spec question | Resolution |
|---|---|
| Q1 own-`uuid` columns | **KEPT** (×6) — they are the REST/MCP CRUD key. Frontend should switch to bigint `id`; only the session uuid is operator-meaningful. |
| Q2 `_ref` → `_id` | **DONE** — 0 `*_ref` columns. |
| Q3 `firing_rule_ids` normalise | left as `uuid[]` — see decision #4. |
| Q4 de-partition `requests` | **DONE** — single table. |

---

### agent_dispatches

**Planned → Done:** drop text `tool_use_id`+`tool_use_ref` → bigint FK [CASCADE] ✓ · drop uuid `agent_id`+`agent_ref` → bigint FK [RESTRICT] ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`agent_dispatches_pkey`) | PK index | ✓ |
| tool_use_id | bigint | NOT NULL | — | UNIQUE (`agent_dispatches_tool_use_id_key`) · FK→tool_uses.id [CASCADE] | unique index | ✓ 1:1 to tool_uses |
| agent_id | bigint | NOT NULL | — | FK→agents.id [RESTRICT] | `idx_agent_dispatches_agent_id` | ✓ |

### agent_instances

**Planned → Done:** session/agent collapse to bigint FKs ✓ · `dispatch_id` UNIQUE FK ✓ · `first_request_id` FK→requests.id [SET NULL] added ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`agent_instances_pkey`) | PK index | ✓ |
| session_id | bigint | NOT NULL | — | FK→sessions.id [CASCADE] | `idx_agent_instances_session_id`, `agent_instances_session_persona_idx` | ✓ |
| dispatch_id | bigint | NOT NULL | — | UNIQUE (`agent_instances_dispatch_unique`) · FK→agent_dispatches.id [CASCADE] | unique index | ✓ 1:1 to dispatch |
| agent_id | bigint | yes | — | FK→agents.id [SET NULL] | `idx_agent_instances_agent_id` | ✓ |
| subagent_persona | text | yes | — | — | `agent_instances_session_persona_idx` (partial) | ✓ |
| first_request_id | bigint | yes | — | FK→requests.id [SET NULL] | — | ✓ FK added this session |
| started_at | timestamptz | yes | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### agents

**Planned → Done:** keep `id` PK ✓ · uuid KEPT ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`agents_pkey`) | PK index | ✓ redundant `uq_agents_id` dropped (working tree) |
| uuid | uuid | NOT NULL | gen_random_uuid() | UNIQUE (`uq_agents_uuid`) | unique index | ✓ REST/MCP key |
| name | text | NOT NULL | — | UNIQUE (`agents_name_key`) | `uq_agents_lower_name` (UNIQUE, lower(name)) | ✓ |
| definition_path | text | yes | — | — | — | ✓ |
| description | text | yes | — | — | — | ✓ |
| source | text | NOT NULL | 'observed' | CHECK (observed/file_backed/dynamic) | `idx_agents_source` | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| updated_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### filter_rules

**Planned → Done:** keep `id` PK ✓ · uuid KEPT ✓ · `(kind,value)` composite UNIQUE ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`filter_rules_pkey`) | PK index | ✓ redundant `uq_filter_rules_id` dropped (working tree) |
| uuid | uuid | NOT NULL | — | UNIQUE (`uq_filter_rules_uuid`) | unique index | ✓ REST/MCP key |
| kind | text | NOT NULL | — | UNIQUE (`filter_rules_kind_value_key`, with value) | composite unique | ✓ |
| value | text | NOT NULL | — | UNIQUE (`filter_rules_kind_value_key`, with kind) | composite unique | ✓ |
| action | text | NOT NULL | — | CHECK (drop/replace/append) | — | ✓ |
| text | text | yes | — | — | — | ✓ |
| enabled | boolean | NOT NULL | true | — | — | ✓ |
| source | text | NOT NULL | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| updated_at | timestamptz | NOT NULL | now() | — | — | ✓ |

_Trigger: `trg_filter_rules_row_changed` (NOTIFY, keyed on `uuid`)._

### lessons

**Planned → Done:** session/agent/duplicate_of collapse to bigint FKs ✓ · uuid KEPT ✓ · `request_id` FK→requests.id [SET NULL] added ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`lessons_pkey`) | PK index | ✓ redundant `uq_lessons_id` dropped (working tree) |
| uuid | uuid | NOT NULL | gen_random_uuid() | UNIQUE (`uq_lessons_uuid`) | unique index | ✓ MCP `delete_lesson` key |
| project_id | bigint | yes | — | FK→projects.id [CASCADE] | `idx_lessons_project` + dedup/injectable partials | ✓ |
| source_turn_id | bigint | yes | — | FK→operator_turns.id [SET NULL] | — | ✓ |
| session_id | bigint | yes | — | FK→sessions.id [SET NULL] | `idx_lessons_session_id`, `idx_lessons_session` | ✓ |
| agent_id | bigint | yes | — | FK→agents.id [SET NULL] | `idx_lessons_agent_id` + dedup/injectable partials | ✓ |
| agent_instance_id | bigint | yes | — | FK→agent_instances.id [SET NULL] | `idx_lessons_agent_instance` | ✓ |
| duplicate_of_id | bigint | yes | — | FK→lessons.id [SET NULL] (self) | `idx_lessons_duplicate_of_id` | ✓ |
| request_id | bigint | yes | — | FK→requests.id [SET NULL] | — | ✓ FK added this session |
| kind | text | NOT NULL | — | CHECK (correction/convention/warning/preference) | dedup partials (with text_sha256) | ✓ |
| text | text | NOT NULL | — | — | — | ✓ |
| stage | text | NOT NULL | 'raw' | — | daily/raw/injectable partials | ✓ |
| source | text | NOT NULL | 'agent' | — | — | ✓ |
| text_sha256 | bytea | yes | — | — | dedup partials | ✓ |
| embedding | vector | yes | — | — | `idx_lessons_embedding` (hnsw) | ✓ |
| embedding_model | text | yes | — | — | — | ✓ |
| embedded_at | timestamptz | yes | — | — | `idx_lessons_raw_pending` (partial) | ✓ |
| consolidation_count | integer | NOT NULL | 0 | — | — | ✓ |
| last_consolidated_at | timestamptz | yes | — | — | — | ✓ |
| archived_at | timestamptz | yes | — | — | injectable partials | ✓ |
| archived_reason | text | yes | — | CHECK + consistency CHECK with archived_at | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| expires_at | timestamptz | yes | — | — | — | ✓ |

_Trigger: `trg_lessons_row_changed` (NOTIFY)._

### messages

**Planned → Done:** clean content-addressed dedup ✓ · redundant `UNIQUE (id)` already dropped (`ed5d4d2`) ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`messages_pkey`) | PK index | ✓ |
| message_sha256 | text | NOT NULL | — | UNIQUE (`uq_messages_message_sha256`) · CHECK 64-hex | unique index | ✓ content address |
| body | jsonb | NOT NULL | — | — | — | ✓ |
| byte_size | integer | NOT NULL | — | CHECK ≥ 0 | — | ✓ |
| role | text | NOT NULL | — | CHECK (user/assistant/system) | — | ✓ |
| first_seen | timestamptz | NOT NULL | now() | — | — | ✓ |
| last_seen | timestamptz | NOT NULL | now() | — | `idx_messages_last_seen` | ✓ |

### operator_turn_chunks

**Planned → Done:** clean ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`operator_turn_chunks_pkey`) | PK index | ✓ |
| turn_id | bigint | NOT NULL | — | UNIQUE (`…_turn_id_chunk_role_chunk_idx_key`, +chunk_role+chunk_idx) · FK→operator_turns.id [CASCADE] | `idx_otc_turn` | ✓ |
| chunk_idx | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| chunk_role | text | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| chunk_text | text | NOT NULL | — | — | — | ✓ |
| chunk_embedding | vector | NOT NULL | — | — | `idx_otc_embedding` (hnsw) | ✓ |
| embedding_model | text | NOT NULL | — | — | — | ✓ |
| embedded_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### operator_turns

**Planned → Done:** `session_id` → bigint FK ✓ · composite UNIQUE `(session_id, start_request_id)` ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`operator_turns_pkey`) | PK index | ✓ |
| session_id | bigint | NOT NULL | — | UNIQUE (`uq_operator_turns_session_start`, +start_request_id) · FK→sessions.id [CASCADE] | composite unique (leads) · `operator_turns_one_open_per_session` (partial) | ✓ |
| project_id | bigint | NOT NULL | — | FK→projects.id [CASCADE] | `idx_operator_turns_project` | ✓ |
| start_request_id | bigint | NOT NULL | — | FK→requests.id [CASCADE] | `idx_operator_turns_start_request_id` | ✓ |
| end_request_id | bigint | NOT NULL | — | FK→requests.id [CASCADE] | `idx_operator_turns_end_request_id` | ✓ |
| outcome_evidence_request_id | bigint | yes | — | FK→requests.id [SET NULL] | `idx_operator_turns_evidence_request_id` | ✓ |
| outcome | text | NOT NULL | 'unknown' | — | `idx_operator_turns_outcome_created` | ✓ |
| classified_at | timestamptz | yes | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| closed_at | timestamptz | yes | — | — | `operator_turns_one_open_per_session` (WHERE closed_at IS NULL) | ✓ |
| embedded_at | timestamptz | yes | — | — | — | ✓ |

### projects

**Planned → Done:** clean root ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`projects_pkey`) | PK index · `idx_projects_pinned` (partial) | ✓ |
| project_path | text | NOT NULL | — | UNIQUE (`projects_project_path_key`) | unique index | ✓ natural key |
| display_name | text | NOT NULL | — | — | — | ✓ |
| first_seen | timestamptz | NOT NULL | — | — | — | ✓ |
| last_seen | timestamptz | NOT NULL | — | — | `idx_projects_last_seen` | ✓ |
| pinned | boolean | NOT NULL | false | — | `idx_projects_pinned` (WHERE pinned) | ✓ |
| ttl_override | interval | yes | — | — | — | ✓ |
| pin_created_at | timestamptz | yes | — | — | — | ✓ |
| ttl_override_set_at | timestamptz | yes | — | — | — | ✓ |

### prompt_composition_revisions

**Planned → Done:** `composition_id` → bigint FK ✓ · composite UNIQUE `(composition_id, version)` ✓ · uuid KEPT ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`prompt_composition_revisions_pkey`) | PK index | ✓ |
| uuid | uuid | NOT NULL | gen_random_uuid() | UNIQUE (`uq_pcr_uuid`) | unique index | ✓ |
| composition_id | bigint | NOT NULL | — | UNIQUE (`uq_pcr_composition_version`, +version) · FK→prompt_compositions.id [CASCADE] | `idx_pcr_composition_id` | ✓ |
| version | bigint | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| label | text | NOT NULL | — | — | — | ✓ |
| blocks | jsonb | NOT NULL | — | — | — | ✓ |
| mode | text | NOT NULL | — | — | — | ✓ |
| notes | text | yes | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### prompt_compositions

**Planned → Done:** agent/filter_rule/system_prompt refs → bigint FKs ✓ · uuid KEPT ✓ · `system_prompt_id` NOT NULL

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`prompt_compositions_pkey`) | PK index | ✓ redundant `uq_prompt_compositions_id` dropped (working tree) |
| uuid | uuid | NOT NULL | gen_random_uuid() | UNIQUE (`uq_prompt_compositions_uuid`) | unique index | ✓ REST/MCP key |
| prompt_sha256 | text | NOT NULL | — | UNIQUE (`prompt_compositions_prompt_sha256_key`) | unique index | ✓ |
| agent_id | bigint | yes | — | FK→agents.id [SET NULL] | `idx_prompt_compositions_agent_id` | ✓ |
| filter_rule_id | bigint | yes | — | FK→filter_rules.id [CASCADE] | `idx_prompt_compositions_filter_rule_id` | ✓ |
| system_prompt_id | bigint | NOT NULL | — | FK→system_prompts.id [NO ACTION] | `idx_prompt_compositions_sysprompt_id` | ✓ |
| agent_name | text | yes | — | — | `idx_pc_agent_name` (partial) | ✓ |
| agent_definition_path | text | yes | — | — | — | ✓ |
| label | text | NOT NULL | — | — | — | ✓ |
| blocks | jsonb | NOT NULL | — | — | — | ✓ |
| mode | text | NOT NULL | 'observe' | CHECK (observe/template/replace) | — | ✓ |
| notes | text | yes | — | — | — | ✓ |
| version | bigint | NOT NULL | 1 | — | — | ✓ |
| source | text | NOT NULL | 'manual' | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| updated_at | timestamptz | NOT NULL | now() | — | — | ✓ |

_Trigger: `trg_prompt_compositions_row_changed` (NOTIFY, keyed on `uuid`)._

### request_deltas

**Planned → Done:** `session_id` → bigint FK ✓ · `predecessor_request_id` FK→requests.id [SET NULL] added ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_deltas_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_request_deltas_request_change_ord`, +change_kind+ord) · FK→requests.id [CASCADE] | `idx_request_deltas_request_id` | ✓ |
| predecessor_request_id | bigint | yes | — | FK→requests.id [SET NULL] | — | ✓ FK added this session |
| session_id | bigint | NOT NULL | — | FK→sessions.id [CASCADE] | `idx_request_deltas_session_id` | ✓ |
| message_id | bigint | NOT NULL | — | FK→messages.id [NO ACTION] | `idx_request_deltas_message_id` | ✓ |
| change_kind | text | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| message_sha256 | text | NOT NULL | — | — | — | ⚠ #2 derivable via `message_id` FK — drop candidate |
| role | text | NOT NULL | — | — | `idx_request_deltas_role` | ✓ |
| byte_size | integer | NOT NULL | — | — | — | ✓ |
| body | jsonb | NOT NULL | — | — | — | ✓ |
| removed_reason | text | yes | — | — | `idx_request_deltas_removed_reason` (partial) | ✓ |
| computed_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### request_diagnostics

**Planned → Done:** clean ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_diagnostics_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | FK→requests.id [CASCADE] | `idx_request_diagnostics_request_id` | ✓ |
| category | text | NOT NULL | — | — | `idx_request_diagnostics_category_created_at` | ✓ |
| analyzer | text | NOT NULL | — | — | `idx_request_diagnostics_analyzer_created_at` | ✓ |
| advice | text | NOT NULL | — | — | — | ✓ |
| matched_text | text | NOT NULL | — | — | — | ✓ |
| match_range | jsonb | yes | — | — | — | ✓ |
| other_hits | jsonb | NOT NULL | '[]' | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### request_messages

**Planned → Done:** `message_ref` → `message_id` bigint FK ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_messages_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_request_messages_request_ord`, +ord) · FK→requests.id [CASCADE] | `idx_request_messages_request_id` | ✓ |
| message_id | bigint | NOT NULL | — | FK→messages.id [NO ACTION] | `idx_request_messages_message_id` | ✓ |
| ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| message_sha256 | text | NOT NULL | — | — | `idx_request_messages_sha` | ⚠ #2 derivable via `message_id` FK — drop candidate (sha index reads it today) |

### request_responses

**Planned → Done:** clean; surrogate `id` PK kept ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_responses_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_request_responses_request`) · FK→requests.id [CASCADE] | `idx_request_responses_request_id` | ✓ 1:1 |
| response_status | smallint | NOT NULL | — | — | — | ✓ |
| response_headers | jsonb | NOT NULL | '[]' | — | — | ✓ |
| response_body | bytea | NOT NULL | '\x' | — | — | ✓ |
| upstream_request_id | text | yes | — | — | `idx_request_responses_upstream_request_id` (partial) | ⓘ external Anthropic id (not a local FK) |
| upstream_message_id | text | yes | — | — | — | ⓘ external Anthropic `msg_…` (not a local FK) |
| token_usage | jsonb | NOT NULL | '{}' | — | — | ✓ |
| stop_reason | text | yes | — | — | `idx_request_responses_stop_reason` (partial) | ✓ |
| stop_details | jsonb | yes | — | — | — | ✓ |
| completed_at | timestamptz | NOT NULL | — | — | `idx_request_responses_completed_at` (brin) | ✓ |
| persisted_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### request_system_prompts

**Planned → Done:** composition/system_prompt refs → bigint FKs ✓ · `firing_rule_ids` left as `uuid[]`

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_system_prompts_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_request_system_prompts_request_id`) · FK→requests.id [CASCADE] | unique index | ✓ 1:1 |
| prompt_sha256 | text | yes | — | — | `idx_rsp_prompt_sha256` (partial) | ✓ |
| composition_id | bigint | yes | — | FK→prompt_compositions.id [NO ACTION] | `idx_request_system_prompts_composition_id` | ✓ |
| system_prompt_id | bigint | yes | — | FK→system_prompts.id [NO ACTION] | `idx_request_system_prompts_sysprompt_id` | ✓ |
| composition_applied | boolean | NOT NULL | false | — | — | ✓ |
| unknown_blocks | jsonb | yes | — | — | — | ✓ |
| filter_modified | boolean | NOT NULL | false | — | — | ✓ |
| firing_rule_ids | uuid[] | NOT NULL | '{}' | — (array of `filter_rules.uuid`, no FK possible) | `idx_rsp_firing_rule_ids` (gin, partial) | ⚠ #4 should be id-based → `bigint[]` or junction table |

### request_tools

**Planned → Done:** clean ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`request_tools_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_request_tools_request_ord`, +ord) · FK→requests.id [CASCADE] | `idx_request_tools_request` | ✓ |
| ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| tool_name | text | NOT NULL | — | — | `idx_request_tools_name` | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### requests

**Planned → Done:** `session_id` → bigint FK [CASCADE] ✓ · de-partitioned ✓ · `agent_instance_id` FK→agent_instances.id [SET NULL] present ✓ · `project_id` kept NO ACTION

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`requests_pkey`) | PK index | ✓ |
| session_id | bigint | NOT NULL | — | FK→sessions.id [CASCADE] | `idx_requests_session_id`, `idx_requests_inflight_session`, `idx_requests_session_completed` | ✓ |
| project_id | bigint | NOT NULL | — | FK→projects.id [NO ACTION] | `idx_requests_project_created_desc` | ✓ |
| method | text | NOT NULL | — | — | — | ✓ |
| path | text | NOT NULL | — | — | — | ✓ |
| model | text | yes | — | — | `idx_requests_model_created_at` (partial) | ✓ |
| is_streaming | boolean | NOT NULL | — | — | — | ✓ |
| message_count | integer | NOT NULL | — | — | — | ✓ |
| is_tool_loop | boolean | NOT NULL | — | — | — | ✓ |
| status | text | NOT NULL | — | — | `idx_requests_status_created_at`, `idx_requests_inflight_session` | ✓ |
| request_headers | jsonb | yes | — | — | — | ✓ |
| params | jsonb | NOT NULL | — | — | — | ✓ |
| last_user_message | text | NOT NULL | — | — | — | ✓ |
| human_prompt | text | NOT NULL | — | — | — | ✓ |
| turn_type | text | NOT NULL | — | — | — | ✓ |
| turn_origin | text | NOT NULL | — | — | `idx_requests_turn_origin_created_at` | ✓ |
| subagent_persona | text | yes | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | — | — | `brin_requests_created_at` | ✓ |
| completed_at | timestamptz | yes | — | — | `idx_requests_session_completed` (partial) | ✓ |
| persisted_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| request_body | bytea | yes | — | — | — | ✓ |
| filtered_request_body | bytea | yes | — | — | — | ✓ |
| intercepted | boolean | NOT NULL | false | — | — | ✓ |
| correction_request_id | bigint | yes | — | FK→requests.id [SET NULL] (self) | `idx_requests_correction_request_id` (partial) | ✓ |
| agent_instance_id | bigint | yes | — | FK→agent_instances.id [SET NULL] | `requests_agent_instance_id_idx` (partial) | ✓ FK present |

_Trigger: `trg_requests_row_changed` (NOTIFY; resolves session string from `sessions`)._

### response_stream_chunks

**Planned → Done:** `session_id` → bigint FK ✓ (denormalised session FK kept)

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`response_stream_chunks_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_response_stream_chunks_request_seq`, +seq_ord) · FK→requests.id [CASCADE] | `idx_response_stream_chunks_request_id`, `…_request_delta_type` | ✓ |
| session_id | bigint | NOT NULL | — | FK→sessions.id [CASCADE] | `idx_response_stream_chunks_session_id` | ✓ |
| seq_ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| chunk_content | text | NOT NULL | — | — | — | ✓ |
| chunk_byte_size | integer | NOT NULL | — | — | — | ✓ |
| chunk_sha256 | text | NOT NULL | — | — | — | ✓ |
| delta_type | text | NOT NULL | — | — | `…_request_delta_type` (composite) | ✓ |
| arrived_at | timestamptz | NOT NULL | now() | — | `idx_response_stream_chunks_arrived_at` (brin) | ✓ |

### sessions

**Planned → Done:** keep `id` PK + `session_id` text UNIQUE ✓ · `project_id` FK→projects.id [CASCADE] present ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`sessions_pkey`) | PK index | ✓ redundant `uq_sessions_id` dropped (working tree) |
| session_id | text | NOT NULL | — | UNIQUE (`sessions_session_id_key`) | `idx_sessions_pinned` (partial), `idx_sessions_session_id_trgm` (gin) | ⓘ text natural key (rule 3) · #6 uuid-typing is a separate decision |
| project_id | bigint | NOT NULL | — | FK→projects.id [CASCADE] | `idx_sessions_project_last_seen` | ✓ FK present |
| pinned | boolean | NOT NULL | false | — | `idx_sessions_pinned` (WHERE pinned) | ✓ |
| ttl_override | interval | yes | — | — | — | ✓ |
| first_seen | timestamptz | NOT NULL | — | — | — | ✓ |
| last_seen | timestamptz | NOT NULL | — | — | `brin_sessions_last_seen`, `idx_sessions_project_last_seen` | ✓ |
| request_count | integer | NOT NULL | 0 | — | — | ✓ |
| last_path | text | yes | — | — | — | ✓ |
| last_status | text | yes | — | — | — | ✓ |
| pin_created_at | timestamptz | yes | — | — | — | ✓ |
| ttl_override_set_at | timestamptz | yes | — | — | — | ✓ |
| last_models | jsonb | NOT NULL | '[]' | — | — | ✓ |
| title | text | yes | — | — | `idx_sessions_title_trgm` (gin, partial) | ✓ |

_Trigger: `trg_sessions_row_changed` (NOTIFY, keyed on `session_id` string)._

### system_prompts

**Planned → Done:** clean content-addressed ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`system_prompts_pkey`) | PK index | ✓ redundant `uq_system_prompts_id` dropped (working tree) |
| prompt_sha256 | text | NOT NULL | — | UNIQUE (`uq_system_prompts_prompt_sha256`) · CHECK 64-hex | unique index | ✓ content address; request link is on `request_system_prompts` (by design) |
| blocks | jsonb | NOT NULL | — | — | — | ✓ |
| first_seen_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| last_seen_at | timestamptz | NOT NULL | now() | CHECK (last ≥ first) | — | ✓ |

### tool_results

**Planned → Done:** text `tool_use_id`+`tool_use_ref` → bigint FK [CASCADE] ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`tool_results_pkey`) | PK index | ✓ |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_tool_results_request_ord`, +ord) · FK→requests.id [CASCADE] | `idx_tool_results_request_id`, `idx_tool_results_errors` (partial) | ✓ |
| tool_use_id | bigint | NOT NULL | — | FK→tool_uses.id [CASCADE] | `idx_tool_results_tool_use_id` | ✓ |
| ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| is_error | boolean | NOT NULL | — | — | `idx_tool_results_errors` (WHERE is_error) | ✓ |
| content | jsonb | NOT NULL | — | — | — | ✓ |
| content_sha256 | text | NOT NULL | — | — | — | ✓ |
| content_bytes | bigint | NOT NULL | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### tool_uses

**Planned → Done:** keep `id` PK + `tool_id` text UNIQUE ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`tool_uses_pkey`) | PK index | ✓ redundant `uq_tool_uses_id` dropped (working tree) |
| tool_id | text | NOT NULL | — | UNIQUE (`uq_tool_uses_tool_id`) | unique index | ⓘ external Anthropic `toolu_…` id — natural key, no `tools` table to link to |
| request_id | bigint | NOT NULL | — | UNIQUE (`uq_tool_uses_request_ord`, +ord) · FK→requests.id [CASCADE] | `idx_tool_uses_request_id` | ✓ |
| ord | integer | NOT NULL | — | part of composite UNIQUE | — | ✓ |
| tool_name | text | NOT NULL | — | — | `idx_tool_uses_name`, `idx_tool_uses_file_path` (partial), `idx_tool_uses_bash_command_trgm` (partial) | ✓ |
| input | jsonb | yes | — | — | `idx_tool_uses_file_path`, `idx_tool_uses_bash_command_trgm` (functional partials) | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |

### volatile_line_patterns

**Planned → Done:** keep `id` PK ✓ · uuid KEPT ✓ · `name` UNIQUE ✓

| column | type | null | default | key / identity | indexes | status |
|---|---|---|---|---|---|---|
| id | bigint | NOT NULL | — | **PK · IDENTITY** (`volatile_line_patterns_pkey`) | PK index | ✓ (never had a redundant UNIQUE(id)) |
| uuid | uuid | NOT NULL | gen_random_uuid() | UNIQUE (`uq_volatile_line_patterns_uuid`) | unique index | ✓ |
| name | text | NOT NULL | — | UNIQUE (`volatile_line_patterns_name_key`) | `uq_volatile_line_patterns_lower_name` (UNIQUE, lower(name)) | ✓ |
| pattern | text | NOT NULL | — | — | — | ✓ |
| applies_to_block_index | integer | NOT NULL | — | — | — | ✓ |
| enabled | boolean | NOT NULL | true | — | — | ✓ |
| notes | text | yes | — | — | — | ✓ |
| created_at | timestamptz | NOT NULL | now() | — | — | ✓ |
| updated_at | timestamptz | NOT NULL | now() | — | — | ✓ |

---

### backfill_messages_progress — DROPPED

Planned: likely drop (singleton backfill cursor). Done: **dropped** — not in the squashed schema. ✓

---

## Views (in `…05_views.sql`)

`file_touches_v` · `operator_turn_aggregates_v` · `operator_turn_text_v` · `project_fingerprint_v` · `session_summary_v`
