# Migration Squash — Per-Table Target Spec

**Purpose:** collapse all 104 migrations into ONE clean schema — one bigint `id` identity per table, real foreign keys, no transitional cruft. This doc is the source of truth; the squash is built from the **NOTES** under each table.

## Locked rules (settled — do not re-ask)
1. **One identity per table:** `id bigint GENERATED ALWAYS AS IDENTITY` is the PK and the single surrogate key.
2. **`_ref` → `_id`:** every foreign key is a bigint column named `<parent>_id` referencing `<parent>.id`. The transitional `*_ref` columns and their legacy text/uuid twins collapse into that single bigint FK. **No `_ref` columns survive.**
3. **session:** the session string is load-bearing and lives **only on `sessions`** (`sessions.session_id` text, UNIQUE). On every child, the session reference is a single bigint **`session_id` FK → `sessions.id`** (today's `session_id` text + `session_ref` collapse into it).
4. **Owning-table natural keys stay** as UNIQUE: `sessions.session_id` (text), `tool_uses.tool_id` (text). Other legacy own-`uuid` columns (agents, filter_rules, prompt_compositions, …) are an open question — see Q1.

**Reading the tables:** `PK`=primary key · `U`=unique · `U(a,b)`=composite unique on those columns together · `FK→t.id [RULE]`=foreign key + on-delete (CASCADE/SETNULL/NOACTION/RESTRICT). **→ Target** = the proposed end state from the locked rules; override it in **NOTES**.

Source: live DB `claude_proxy_test_template`, 2026-06-04.

---

### agents
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| name | text | U |
| definition_path | text | |
| description | text | |
| source | text | |
| created_at | timestamptz | |
| updated_at | timestamptz | |

**→ Target:** keep `id` PK. `uuid` = keep or drop? (Q1). No incoming legacy refs (children point via `agent_id` bigint).

**NOTES:**
>

---

### agent_dispatches
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| tool_use_id | text | U |
| tool_use_ref | bigint | U, FK→tool_uses.id [CASCADE] |
| agent_id | uuid | |
| agent_ref | bigint | FK→agents.id [RESTRICT] |

**→ Target:** drop text `tool_use_id` + drop `tool_use_ref`; add `tool_use_id` bigint FK→tool_uses.id. Drop uuid `agent_id` + `agent_ref`; add `agent_id` bigint FK→agents.id.

**NOTES:**
>

---

### agent_instances
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| session_id | text | |
| session_ref | bigint | FK→sessions.id [CASCADE] |
| dispatch_id | bigint | U, FK→agent_dispatches.id [CASCADE] |
| agent_id | uuid | |
| agent_ref | bigint | FK→agents.id [SETNULL] |
| subagent_persona | text | |
| first_request_id | bigint | (relates to requests; no FK) |
| started_at | timestamptz | |
| created_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id. Collapse uuid `agent_id` + `agent_ref` → `agent_id` bigint FK→agents.id. `dispatch_id` already clean. `first_request_id` → add FK→requests.id?

**NOTES:**
>

---

### backfill_messages_progress
| column | type | key / fk |
|---|---|---|
| id | bigint | PK (singleton) |
| last_processed_request_id | bigint | (relates to requests; no FK) |
| last_processed_created_at | timestamptz | |
| started_at | timestamptz | |
| updated_at | timestamptz | |
| completed_at | timestamptz | |

**→ Target:** likely DROP the whole table — it's a resume-cursor for a one-time messages backfill; a squashed fresh schema has nothing to backfill. Confirm before dropping.

**NOTES:**
>whats the point of this table?
>↳ (claude): singleton cursor for a one-time data-backfill job — tracks how far the backfill got so it can resume after a restart. Vestigial in a fresh squashed schema → propose drop.

---

### filter_rules
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| kind | text | U(kind, value) |
| value | text | U(kind, value) |
| action | text | |
| text | text | |
| enabled | bool | |
| source | text | |
| created_at | timestamptz | |
| updated_at | timestamptz | |

**→ Target:** keep `id` PK. `uuid` = keep or drop? (Q1). The `(kind, value)` composite-UNIQUE stays (dedup key). The FK *into* filter_rules lives on prompt_compositions — see its `filter_rule_ref` → becomes `filter_rule_id`.

**NOTES:**
>`kind`+`value` txt U(kind,value)  ? what are you saiyng?
>↳ (claude): `U(kind,value)` = ONE composite UNIQUE constraint over the pair (kind, value) — no two rules can share the same kind+value combination. Now shown as two rows both tagged `U(kind, value)`.
>filter_rule_ref should be filter_rule_id.
>↳ (claude): agreed — that's the `_ref`→`_id` rule. (That column is on prompt_compositions; renamed there.)

---

### lessons
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| project_id | bigint | FK→projects.id [CASCADE] |
| source_turn_id | bigint | FK→operator_turns.id [SETNULL] |
| session_id | text | |
| session_ref | bigint | FK→sessions.id [SETNULL] |
| agent_id | uuid | |
| agent_ref | bigint | FK→agents.id [SETNULL] |
| agent_instance_id | bigint | FK→agent_instances.id [SETNULL] |
| duplicate_of_id | uuid | |
| duplicate_of_ref | bigint | FK→lessons.id [SETNULL] (self) |
| request_id | bigint | (relates to requests; no FK) |
| kind | text | |
| text | text | |
| stage | text | |
| source | text | |
| text_sha256 | bytea | |
| embedding | vector | |
| embedding_model | text | |
| embedded_at | timestamptz | |
| consolidation_count | int | |
| last_consolidated_at | timestamptz | |
| archived_at | timestamptz | |
| archived_reason | text | |
| created_at | timestamptz | |
| expires_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id. Collapse uuid `agent_id` + `agent_ref` → `agent_id` bigint FK→agents.id. Collapse uuid `duplicate_of_id` + `duplicate_of_ref` → `duplicate_of_id` bigint FK→lessons.id. `request_id` → add FK→requests.id? `uuid` = keep or drop? (Q1).

**NOTES:**
>source_turn_id how can this be null?
>↳ (claude): it's nullable + ON DELETE SET NULL so a lesson can exist without an originating turn (manual/agent-authored, or the turn was later deleted). If every lesson MUST come from a turn, say so and I make it NOT NULL.
>session_id should be pointing to session.id
>↳ (claude): locked — `session_id` becomes a bigint FK → sessions.id (rule 3).

---

### messages
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| message_sha256 | text | U |
| body | jsonb | |
| byte_size | int | |
| role | text | |
| first_seen | timestamptz | |
| last_seen | timestamptz | |

**→ Target:** clean already (content-addressed dedup table). Children reference it via `message_id` bigint.

**NOTES:**
>

---

### operator_turn_chunks
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| turn_id | bigint | U(turn_id, chunk_role, chunk_idx), FK→operator_turns.id [CASCADE] |
| chunk_idx | int | |
| chunk_role | text | |
| chunk_text | text | |
| chunk_embedding | vector | |
| embedding_model | text | |
| embedded_at | timestamptz | |

**→ Target:** clean already.

**NOTES:**
>

---

### operator_turns
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| session_id | text | |
| session_ref | bigint | U(session_ref, start_request_id), FK→sessions.id [CASCADE] |
| project_id | bigint | FK→projects.id [CASCADE] |
| start_request_id | bigint | FK→requests.id [CASCADE] |
| end_request_id | bigint | FK→requests.id [CASCADE] |
| outcome_evidence_request_id | bigint | FK→requests.id [SETNULL] |
| outcome | text | |
| classified_at | timestamptz | |
| created_at | timestamptz | |
| closed_at | timestamptz | |
| embedded_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id (composite UNIQUE becomes `(session_id, start_request_id)`). Rest clean.

**NOTES:**
>

---

### projects
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| project_path | text | U |
| display_name | text | |
| first_seen | timestamptz | |
| last_seen | timestamptz | |
| pinned | bool | |
| ttl_override | interval | |
| pin_created_at | timestamptz | |
| ttl_override_set_at | timestamptz | |

**→ Target:** clean root. `project_path` is its natural UNIQUE key.

**NOTES:**
>

---

### prompt_compositions
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| prompt_sha256 | text | U |
| agent_id | uuid | |
| agent_ref | bigint | FK→agents.id [SETNULL] |
| linked_filter_rule_id | uuid | |
| filter_rule_ref | bigint | FK→filter_rules.id [CASCADE] |
| system_prompt_ref | bigint | FK→system_prompts.id [NOACTION] |
| agent_name | text | |
| agent_definition_path | text | |
| label | text | |
| blocks | jsonb | |
| mode | text | |
| notes | text | |
| version | bigint | |
| source | text | |
| created_at | timestamptz | |
| updated_at | timestamptz | |

**→ Target:** collapse uuid `agent_id`+`agent_ref` → `agent_id` bigint FK→agents.id. Collapse uuid `linked_filter_rule_id`+`filter_rule_ref` → `filter_rule_id` bigint FK→filter_rules.id. `system_prompt_ref` → `system_prompt_id` bigint FK→system_prompts.id. `uuid` = keep or drop? (Q1).

**NOTES:**
>

---

### prompt_composition_revisions
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| composition_id | uuid | |
| composition_ref | bigint | U(composition_ref, version), FK→prompt_compositions.id [CASCADE] |
| version | bigint | |
| label | text | |
| blocks | jsonb | |
| mode | text | |
| notes | text | |
| created_at | timestamptz | |

**→ Target:** collapse uuid `composition_id`+`composition_ref` → `composition_id` bigint FK→prompt_compositions.id (composite UNIQUE → `(composition_id, version)`). `uuid` = keep or drop? (Q1).

**NOTES:**
>

---

### request_deltas
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U(request_id, change_kind, ord), FK→requests.id [CASCADE] |
| predecessor_request_id | bigint | (relates to requests; no FK) |
| message_id | bigint | FK→messages.id [NOACTION] |
| session_id | text | |
| session_ref | bigint | FK→sessions.id [CASCADE] |
| change_kind | text | |
| ord | int | |
| message_sha256 | text | |
| role | text | |
| byte_size | int | |
| body | jsonb | |
| removed_reason | text | |
| computed_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id. `message_id` already clean. `predecessor_request_id` → add FK→requests.id? `message_sha256` redundant with `message_id`?

**NOTES:**
>

---

### request_diagnostics
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | FK→requests.id [CASCADE] |
| category | text | |
| analyzer | text | |
| advice | text | |
| matched_text | text | |
| match_range | jsonb | |
| other_hits | jsonb | |
| created_at | timestamptz | |

**→ Target:** clean.

**NOTES:**
>

---

### request_messages
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U(request_id, ord), FK→requests.id [CASCADE] |
| message_ref | bigint | FK→messages.id [NOACTION] |
| ord | int | |
| message_sha256 | text | |

**→ Target:** rename `message_ref` → `message_id` bigint FK→messages.id. `message_sha256` text redundant with the FK? (keep for content lookup, or drop?)

**NOTES:**
>

---

### request_responses
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U, FK→requests.id [CASCADE] (1:1) |
| response_status | smallint | |
| response_headers | jsonb | |
| response_body | bytea | |
| upstream_request_id | text | |
| upstream_message_id | text | |
| token_usage | jsonb | |
| stop_reason | text | |
| stop_details | jsonb | |
| completed_at | timestamptz | |
| persisted_at | timestamptz | |

**→ Target:** clean (surrogate `id` + UNIQUE `request_id`). Could drop `id` and make `request_id` the PK (1:1) — your call.

**NOTES:**
>

---

### request_system_prompts
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U, FK→requests.id [CASCADE] (1:1) |
| composition_id | uuid | |
| composition_ref | bigint | FK→prompt_compositions.id [NOACTION] |
| prompt_sha256 | text | |
| system_prompt_ref | bigint | FK→system_prompts.id [NOACTION] |
| composition_applied | bool | |
| unknown_blocks | jsonb | |
| filter_modified | bool | |
| firing_rule_ids | uuid[] | |

**→ Target:** collapse uuid `composition_id`+`composition_ref` → `composition_id` bigint FK→prompt_compositions.id. `system_prompt_ref` → `system_prompt_id` bigint FK→system_prompts.id. `firing_rule_ids uuid[]` → normalise to a join table? (Q3).

**NOTES:**
>

---

### request_tools
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U(request_id, ord), FK→requests.id [CASCADE] |
| ord | int | |
| tool_name | text | |
| created_at | timestamptz | |

**→ Target:** clean.

**NOTES:**
>

---

### requests
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| session_id | text | |
| session_ref | bigint | FK→sessions.id [CASCADE] |
| project_id | bigint | FK→projects.id [NOACTION] |
| correction_request_id | bigint | FK→requests.id [SETNULL] (self) |
| agent_instance_id | bigint | FK→agent_instances.id [SETNULL] |
| method / path / model / status | text | |
| turn_type / turn_origin / subagent_persona | text | |
| is_streaming / is_tool_loop / intercepted | bool | |
| message_count | int | |
| request_headers / params | jsonb | |
| last_user_message / human_prompt | text | |
| request_body / filtered_request_body | bytea | |
| created_at / completed_at / persisted_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id. This is the big consumer rewire (~1825 sites). `project_id` FK should probably be CASCADE/SETNULL not NOACTION — confirm.

**NOTES:**
>

---

### response_stream_chunks
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U(request_id, seq_ord), FK→requests.id [CASCADE] |
| session_id | text | |
| session_ref | bigint | FK→sessions.id [CASCADE] |
| seq_ord | int | |
| chunk_content | text | |
| chunk_byte_size | int | |
| chunk_sha256 | text | |
| delta_type | text | |
| arrived_at | timestamptz | |

**→ Target:** collapse `session_id` text + `session_ref` → `session_id` bigint FK→sessions.id. (Does this table even need a denormalised session at all, given request_id→requests already carries it? Consider dropping the session FK here.)

**NOTES:**
>

---

### sessions
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| session_id | text | U — **load-bearing natural key, STAYS** |
| project_id | bigint | FK→projects.id [CASCADE] |
| pinned | bool | |
| ttl_override | interval | |
| first_seen / last_seen | timestamptz | |
| pin_created_at / ttl_override_set_at | timestamptz | |
| request_count | int | |
| last_path / last_status / title | text | |
| last_models | jsonb | |

**→ Target:** keep `id` PK + `session_id` text UNIQUE. This is the ONLY place the session string lives.

**NOTES:**
>

---

### system_prompts
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| prompt_sha256 | text | U |
| blocks | jsonb | |
| first_seen_at | timestamptz | |
| last_seen_at | timestamptz | |

**→ Target:** clean (content-addressed). Children reference via `system_prompt_id` bigint.

**NOTES:**
>

---

### tool_results
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| request_id | bigint | U(request_id, ord), FK→requests.id [CASCADE] |
| tool_use_id | text | |
| tool_use_ref | bigint | FK→tool_uses.id [NOACTION] |
| ord | int | |
| is_error | bool | |
| content | jsonb | |
| content_sha256 | text | |
| content_bytes | bigint | |
| created_at | timestamptz | |

**→ Target:** collapse text `tool_use_id` + `tool_use_ref` → `tool_use_id` bigint FK→tool_uses.id [CASCADE]. (Drops the fill-trigger.)

**NOTES:**
>

---

### tool_uses
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| tool_id | text | U — natural key (Anthropic `toolu_…`) |
| request_id | bigint | U(request_id, ord), FK→requests.id [CASCADE] |
| ord | int | |
| tool_name | text | |
| input | jsonb | |
| created_at | timestamptz | |

**→ Target:** keep `id` PK + `tool_id` text UNIQUE (natural key). Children reference via `tool_use_id` bigint.

**NOTES:**
>

---

### volatile_line_patterns
| column | type | key / fk |
|---|---|---|
| id | bigint | PK |
| uuid | uuid | U |
| name | text | U |
| pattern | text | |
| applies_to_block_index | int | |
| enabled | bool | |
| notes | text | |
| created_at | timestamptz | |
| updated_at | timestamptz | |

**→ Target:** keep `id` PK. `uuid` = keep or drop? (Q1). `name` is a natural UNIQUE key.

**NOTES:**
>

---

## Cross-cutting questions

1. **Legacy own-`uuid` columns** (agents, filter_rules, prompt_compositions, prompt_composition_revisions, lessons, volatile_line_patterns): keep as the API-facing identifier, or drop in favour of `id`? Some are exposed in REST/MCP today.
2. **`_ref` → `_id` rename**: CONFIRMED (rule 2).
3. **`firing_rule_ids uuid[]`** (request_system_prompts): array of filter_rule uuids, no FK. Leave, or normalise to a join table?
4. **De-partition `requests`** + **DB-consolidation grep gate** (zero SQL outside the persistence layer) were also part of the original objective — fold into the squash, or track separately?
