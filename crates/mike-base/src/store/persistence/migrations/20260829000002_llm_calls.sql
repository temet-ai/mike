-- LLM call cost recording — artifacts/2026-08-29/llm-cost-and-signal-plan.md,
-- "Recording what calls cost". A single steerer verdict on a reasoning-heavy
-- model measured about nine cents to return a 137-character nudge, and there
-- was no per-call record anywhere to explain where the spend went. This adds
-- one row per outbound OpenRouter call.
--
-- Schema choice: four independent callers write here — the steerer brain,
-- the session judge, the turn judge worker, and the lesson-relations
-- proposer (all under crates/mike/src/llm/) — and none of those subsystems
-- owns this data, so it does not belong under `steering` or
-- `session_judges`. This repo already gives each subsystem its own schema
-- (steering, session_judges, workspace, sync, mailbox, config); `llm` is
-- the schema for this one, and the table is named to match that pattern
-- (`steering.evaluations`, `session_judges.verdicts` — the schema carries
-- the subsystem name once, the table does not repeat it).
--
-- session_id, steerer_id and operator_turn_id are deliberately plain
-- `bigint` with NO foreign key. An outbound call can happen, and this row
-- must still be recorded, before or after the row it refers to exists (a
-- steerer call evaluating a still-open operator_turn; a session judged
-- after its rows have aged out). A foreign key here would let a recording
-- failure fail the call it is trying to record, which the plan explicitly
-- rules out: "Failures to record must never fail the call; log and
-- continue."

CREATE SCHEMA IF NOT EXISTS llm;

CREATE TABLE IF NOT EXISTS llm.calls (
    id                 bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at         timestamptz NOT NULL DEFAULT now(),
    -- Closed set — the four current LLM callers. A fifth caller extends
    -- this CHECK in the migration that adds it, matching
    -- steering.steerers.kind / steering.evaluations.status.
    role               text NOT NULL
                         CHECK (role IN ('steerer', 'session_judge', 'judge_worker', 'lesson_relations')),
    model              text NOT NULL,
    -- OpenRouter's response `id`. A fingerprint for cross-referencing one
    -- row against the OpenRouter dashboard, never a lookup key — per
    -- .claude/rules/sql-set-based.md this is deliberately NOT indexed
    -- (also explicit in the plan: "Do not index the generation id").
    generation_id      text,
    prompt_tokens      integer,
    completion_tokens  integer,
    -- usage.completion_tokens_details.reasoning_tokens — the number that
    -- shows whether the reasoning-effort setting is actually taking
    -- effect; the field that was silently unbounded before this change.
    reasoning_tokens   integer,
    -- usage.prompt_tokens_details.cached_tokens — a cached prompt bills at
    -- a discount; without this a cache hit on an otherwise-identical call
    -- looks like an unexplained price swing.
    cached_tokens      integer,
    -- OpenRouter usage.cost, in USD.
    cost_usd           double precision,
    latency_ms         integer,
    -- Closed set, extended alongside crates/mike/src/llm/error.rs's
    -- LlmError variants (refused_spend backs the new SpendCeilingExceeded
    -- variant).
    outcome            text NOT NULL
                         CHECK (outcome IN ('ok', 'http_error', 'timeout', 'truncated',
                                            'refused_ceiling', 'refused_rate', 'refused_spend')),
    http_status        integer,
    -- Nullable, no FK — see header comment.
    session_id         bigint,
    steerer_id         bigint,
    operator_turn_id   bigint
);

-- llm.calls is append-only time series (one row per outbound call, never
-- updated) — BRIN matches the convention used for requests.requests,
-- steering.interventions and workspace.coordination_events.
CREATE INDEX IF NOT EXISTS idx_llm_calls_created_at
    ON llm.calls USING brin (created_at);

-- Backs the per-role spend rollup behind GET /api/llm/spend (totals by
-- role, plus role-scoped date-range queries).
CREATE INDEX IF NOT EXISTS idx_llm_calls_role_created_at
    ON llm.calls (role, created_at);

COMMENT ON TABLE llm.calls IS
    'One row per outbound OpenRouter call across every LLM caller (steerer, '
    'session_judge, judge_worker, lesson_relations). Written when the '
    'llm.record_usage setting is on; a write failure here must never fail '
    'the call it is recording.';
