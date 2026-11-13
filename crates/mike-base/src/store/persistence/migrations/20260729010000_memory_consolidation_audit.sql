-- Decision-record audit trail for memory consolidation.
--
-- Today a consolidation pass (session/nightly/gc) mutates memory.lessons with
-- no durable record of what it evaluated, how it decided, or whether the
-- surviving lesson ever helped. This adds that record. Design of record:
-- artifacts/2026-07-28/memory-consolidation-audit-plan.md ("The model: a
-- decision record" / "Companion: the injection link").
--
-- Governing shape: runs exist only to group and time; all meaning lives on
-- the action row. There are deliberately NO count columns on
-- consolidation_runs (lessons_evaluated, promoted, archived, ...) -- those
-- answer "how much" when the operator asked "was this done right". Totals
-- are computed on read from consolidation_actions.

-- ── consolidation_runs ──────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS memory.consolidation_run_kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_run_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'session'),
    (2, 'nightly'),
    (3, 'gc')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_run_kinds_id_seq', 3);

CREATE TABLE IF NOT EXISTS memory.consolidation_triggers (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_triggers (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'schedule'),
    (2, 'startup_catchup'),
    (3, 'manual')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_triggers_id_seq', 3);

CREATE TABLE IF NOT EXISTS memory.consolidation_run_statuses (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_run_statuses (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'running'),
    (2, 'completed'),
    (3, 'partial'),
    (4, 'failed')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_run_statuses_id_seq', 4);

COMMENT ON TABLE memory.consolidation_run_statuses IS '''running'' is a durable in-flight marker, not just a terminal label: the run row is inserted at pass-start and updated at pass-end, so a crashed pass shows as a stale running row instead of vanishing. The already-ran-today gate is status IN (completed, partial) so a failed run does not block a retry.';

CREATE TABLE IF NOT EXISTS memory.consolidation_runs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid uuid NOT NULL DEFAULT gen_random_uuid(),
    run_kind_id bigint NOT NULL REFERENCES memory.consolidation_run_kinds(id),
    trigger_id bigint NOT NULL REFERENCES memory.consolidation_triggers(id),
    status_id bigint NOT NULL REFERENCES memory.consolidation_run_statuses(id),
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    error text,
    CONSTRAINT consolidation_runs_finished_consistent
        CHECK ((status_id = 1) = (finished_at IS NULL))
);

COMMENT ON TABLE memory.consolidation_runs IS 'One row per consolidation pass (session/nightly/gc). No count columns by design -- totals are computed on read from consolidation_actions.';
COMMENT ON COLUMN memory.consolidation_runs.finished_at IS 'NULL exactly while status_id = running (enforced by consolidation_runs_finished_consistent); set once the pass completes, partially completes, or fails.';

CREATE INDEX IF NOT EXISTS idx_consolidation_runs_started_at ON memory.consolidation_runs USING btree (started_at DESC);

-- run_kind_id/trigger_id/status_id are NOT indexed: all three point at static
-- lookup tables (3-4 rows) whose rows are never deleted, so the
-- referential-integrity trigger this repo indexes FKs for never fires on
-- them -- see 20260721030000_missing_fk_indexes.sql's closing comment for
-- the load-bearing precedent (indexing a lookup FK "would add ... an index
-- to a ... table for no gain").

-- ── consolidation_actions ───────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS memory.consolidation_action_kinds (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_action_kinds (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'promoted_to_daily'),
    (2, 'promoted_to_long_term'),
    (3, 'archived_duplicate'),
    (4, 'archived_trimmed'),
    (5, 'archived_expired'),
    (6, 'deleted_expired'),
    (7, 'evaluated')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_action_kinds_id_seq', 7);

CREATE TABLE IF NOT EXISTS memory.consolidation_outcomes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_outcomes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'applied'),
    (2, 'failed'),
    (3, 'skipped')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_outcomes_id_seq', 3);

CREATE TABLE IF NOT EXISTS memory.consolidation_decided_by (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.consolidation_decided_by (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'embedding_distance'),
    (2, 'rank_order'),
    (3, 'expiry')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.consolidation_decided_by_id_seq', 3);

CREATE TABLE IF NOT EXISTS memory.consolidation_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id bigint NOT NULL REFERENCES memory.consolidation_runs(id) ON DELETE CASCADE,

    -- Identity & provenance, denormalized on purpose: GC hard-deletes the
    -- lesson row, so a join back to memory.lessons would start from nothing
    -- for that action. The _uuid/_text/files_touched copies are what
    -- survive the delete.
    lesson_id bigint REFERENCES memory.lessons(id) ON DELETE SET NULL,
    lesson_uuid uuid NOT NULL,
    project_id bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    session_uuid uuid,
    source_turn_id bigint REFERENCES memory.operator_turns(id) ON DELETE SET NULL,
    files_touched text[],
    agent_id bigint REFERENCES agents.agents(id) ON DELETE SET NULL,

    -- The content being judged. Full text on EVERY action row, not only
    -- deletes -- this is the "what are you evaluating" answer and the
    -- point of the table. Never sliced (inspection-fidelity layer 1).
    lesson_text text NOT NULL,

    action_kind_id bigint NOT NULL REFERENCES memory.consolidation_action_kinds(id),
    outcome_id bigint NOT NULL REFERENCES memory.consolidation_outcomes(id),
    -- The specific skip/failure cause. Today three distinct skip causes
    -- (embed returned empty / embed errored / neighbour search errored)
    -- collapse into one tracing::warn! line; this column is where they
    -- stop being interchangeable.
    error text,

    before_stage_id bigint REFERENCES memory.lesson_stages(id),
    after_stage_id bigint REFERENCES memory.lesson_stages(id),
    decided_by_id bigint REFERENCES memory.consolidation_decided_by(id),

    -- Measured cosine distance and the threshold in effect AT DECISION TIME
    -- (so a later config change cannot rewrite history) -- both computed
    -- today and discarded.
    distance real,
    threshold real,
    embedding_model text,

    -- The neighbour this lesson was judged against, with its own text so a
    -- human can see both sides of the decision.
    counterpart_lesson_id bigint REFERENCES memory.lessons(id) ON DELETE SET NULL,
    counterpart_uuid uuid,
    counterpart_text text,
    -- Ranks 2-3 of the neighbour set: [{"lesson_id": N, "distance": D}, ...].
    -- find_similar_lessons already fetches 3 candidates and reads only
    -- .first() -- the other two are free evidence with no natural typed
    -- column (a variable-length ranked list), hence jsonb here.
    runners_up jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- For trims only: the consolidation_count/created_at rank and the
    -- excess n that made this row a target. Shape varies by action_kind,
    -- so it isn't a fixed set of typed columns; not filtered/sorted/joined
    -- on, hence jsonb here.
    rank_detail jsonb,

    -- How many times this lesson had been injected as of this decision.
    -- memory.lesson_injections rows die with a hard-deleted lesson; this
    -- is the only place that evidence survives past a GC delete.
    injection_count integer,

    created_at timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT consolidation_actions_outcome_error_consistent
        CHECK ((outcome_id = 1) = (error IS NULL))
);

COMMENT ON TABLE memory.consolidation_actions IS 'One row per lesson per consolidation decision. Denormalized identity/text columns survive the parent lesson''s hard delete (GC); lesson_text carries the FULL text on every row, never sliced.';
COMMENT ON COLUMN memory.consolidation_actions.runners_up IS 'Ranks 2-3 of the compared neighbour set ([{lesson_id, distance}]) -- a variable-length ranked list, not a filter/sort/join target, hence jsonb rather than a bridge table.';
COMMENT ON COLUMN memory.consolidation_actions.rank_detail IS 'Trim-only evidence (consolidation_count/created_at rank, excess n) whose shape varies by action_kind; not queried on, hence jsonb rather than typed columns.';

CREATE INDEX IF NOT EXISTS idx_consolidation_actions_run_id ON memory.consolidation_actions USING btree (run_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_lesson_id ON memory.consolidation_actions USING btree (lesson_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_session_id ON memory.consolidation_actions USING btree (session_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_source_turn_id ON memory.consolidation_actions USING btree (source_turn_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_project_created ON memory.consolidation_actions USING btree (project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_counterpart_lesson_id ON memory.consolidation_actions USING btree (counterpart_lesson_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_agent_id ON memory.consolidation_actions USING btree (agent_id);
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_files_touched ON memory.consolidation_actions USING gin (files_touched);
-- The primary read is the run-agnostic decision feed (GET /api/memory/decisions):
-- every dedup/trim/delete newest-first across ALL runs. That predicate leads
-- on created_at alone, so it needs its own index and cannot ride on the
-- project-scoped composite above.
CREATE INDEX IF NOT EXISTS idx_consolidation_actions_created_at ON memory.consolidation_actions USING btree (created_at DESC);

-- action_kind_id/outcome_id/before_stage_id/after_stage_id/decided_by_id are
-- NOT indexed: same lookup-table rationale as consolidation_runs above --
-- these enum tables top out at 7 rows and are never deleted.

-- ── lesson_injections ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS memory.lesson_injection_lanes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE
);
INSERT INTO memory.lesson_injection_lanes (id, code) OVERRIDING SYSTEM VALUE VALUES
    (1, 'primary'),
    (2, 'subagent')
ON CONFLICT (id) DO NOTHING;
SELECT setval('memory.lesson_injection_lanes_id_seq', 2);

CREATE TABLE IF NOT EXISTS memory.lesson_injections (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    lesson_id bigint NOT NULL REFERENCES memory.lessons(id) ON DELETE CASCADE,
    request_id bigint NOT NULL REFERENCES requests.requests(id) ON DELETE CASCADE,
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    injected_at timestamptz NOT NULL DEFAULT now(),
    lane_id bigint NOT NULL REFERENCES memory.lesson_injection_lanes(id),
    persona text
);

COMMENT ON TABLE memory.lesson_injections IS 'Live attribution: which request got which lesson injected. CASCADE (not SET NULL) on lesson_id/request_id because this table tracks rows that still exist -- unlike consolidation_actions, it is not an audit trail meant to survive its subjects'' deletion; consolidation_actions.injection_count is what carries that evidence past a lesson GC delete.';

CREATE INDEX IF NOT EXISTS idx_lesson_injections_lesson_id ON memory.lesson_injections USING btree (lesson_id);
CREATE INDEX IF NOT EXISTS idx_lesson_injections_request_id ON memory.lesson_injections USING btree (request_id);
CREATE INDEX IF NOT EXISTS idx_lesson_injections_session_injected ON memory.lesson_injections USING btree (session_id, injected_at DESC);

-- lane_id is NOT indexed: 2-row static lookup table, never deleted.
