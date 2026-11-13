-- ════════════════════════════════════════════════════════════════════
-- JC12 — memory.operator_turns.outcome_id becomes a projection with one writer
-- ════════════════════════════════════════════════════════════════════
--
-- Why. Seven writers race for outcome_id today and the first one past the
-- `outcome_id = unknown` guard wins (fleet-mirror J1 §1). After JC12 exactly
-- one SQL statement writes the column: the projection in
-- crates/mike/src/store/persistence/operator_turns/projection.rs, called by
-- promote_turn_verdict. The projection reads the append-only verdict history
-- (memory.turn_verdicts, JC09) and the operator's gold labels (eval.labels,
-- JC07) and writes the legacy code that the existing readers (arming,
-- recall_project_context, list_corrected_turns, verifier_rate_v, eir_timing_v)
-- keep reading unchanged.
--
-- Expand-only. Nothing here drops or retypes a column, so a revert is a
-- binary rollback with no scripted schema rollback (scripts/deploy-proxy.sh:25-36).

-- ── 1. Projection columns on memory.operator_turns ──────────────────────────
--
-- All nullable, no default: adding them is a catalogue-only change on
-- PostgreSQL 11+, and the CHECKs validate against all-NULL values. NULL in
-- outcome_source means "never projected": the row carries a legacy label
-- written by one of the pre-JC12 writers (or the column default, unknown).
--
-- reaction / delivery mirror the closed vocabularies of memory.turn_verdicts
-- (JC09). If JC09 changes either set, the CHECK here changes in the same
-- migration that changes JC09's.

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS reaction text NULL
        CONSTRAINT operator_turns_reaction_check
        CHECK (reaction IS NULL OR reaction IN
               ('corrected', 'redirected', 'accepted', 'no_response', 'unclear')),
    ADD COLUMN IF NOT EXISTS delivery text NULL
        CONSTRAINT operator_turns_delivery_check
        CHECK (delivery IS NULL OR delivery IN
               ('shipped', 'delivered', 'partial', 'none', 'question', 'not_applicable')),
    ADD COLUMN IF NOT EXISTS live_verdict_id bigint NULL
        REFERENCES memory.turn_verdicts(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS outcome_source text NULL
        CONSTRAINT operator_turns_outcome_source_check
        CHECK (outcome_source IS NULL OR outcome_source IN
               ('judge', 'operator', 'frustration_close', 'no_verdict')),
    ADD COLUMN IF NOT EXISTS outcome_projection_version smallint NULL
        CONSTRAINT operator_turns_outcome_projection_version_check
        CHECK (outcome_projection_version IS NULL OR outcome_projection_version >= 1),
    ADD COLUMN IF NOT EXISTS outcome_projected_at timestamptz NULL;

-- 'frustration_close' is in the CHECK now so that JC13 (which adds the
-- frustration branch to the same projection statement) needs no second
-- ALTER of this constraint. JC12's projection (version 1) never writes it.

COMMENT ON COLUMN memory.operator_turns.reaction IS
    'How the operator received this turn, as projected from the live turn verdict or an operator '
    'gold label. NULL until the projection has run for the turn. Written only by project_turn_outcomes.';
COMMENT ON COLUMN memory.operator_turns.delivery IS
    'What the turn delivered against its own prompt, as projected. NULL until projected. '
    'Written only by project_turn_outcomes.';
COMMENT ON COLUMN memory.operator_turns.live_verdict_id IS
    'The memory.turn_verdicts row (mode = live) the current outcome was projected from. NULL when '
    'the source is an operator label, a frustration close without a verdict, or no verdict.';
COMMENT ON COLUMN memory.operator_turns.outcome_source IS
    'Which input decided outcome_id: judge (live verdict), operator (gold label, always wins), '
    'frustration_close (JC13: the session closed in frustration on this, its final genuine turn), '
    'no_verdict (projected to unknown because the turn is not eligible for judging or failed '
    'permanently). NULL: never projected; with outcome_id <> unknown a legacy label from a pre-JC12 '
    'writer (pre-overwrite value kept in memory.operator_turn_legacy_labels), with unknown a turn '
    'still waiting for its verdict.';
COMMENT ON COLUMN memory.operator_turns.outcome_projection_version IS
    'OUTCOME_PROJECTION_VERSION that last wrote this row (it is part of the projection''s change '
    'test, so the next projection of a row written under an older version rewrites it). 1 = JC12 '
    'mapping; 2 = JC13 adds the frustration-close branch. A bump re-projects set-based; it never re-judges.';

CREATE INDEX IF NOT EXISTS idx_operator_turns_live_verdict_id
    ON memory.operator_turns USING btree (live_verdict_id)
    WHERE live_verdict_id IS NOT NULL;

-- ── 2. Frozen copy of every legacy label, taken just before it is overwritten ─
--
-- The r1 and r3 labels are the evidence the fleet-mirror analysis measured
-- (ground_truth.json compares against them). config.settings keeps no history
-- and outcome_id is overwritten in place, so once D9 re-projects the window
-- they would be gone. The projection statement inserts the row here, in the
-- same statement and only when outcome_source IS NULL, before its UPDATE
-- (ON CONFLICT DO NOTHING: one snapshot per turn, the first overwrite wins).
-- No FK to operator_turns: a turn deleted by retention must not take its
-- historical label with it, and the snapshot write must never fail.

CREATE TABLE IF NOT EXISTS memory.operator_turn_legacy_labels (
    turn_id               bigint      PRIMARY KEY,
    outcome_id            bigint      NOT NULL,
    classified_at         timestamptz NULL,
    judge_model           text        NULL,
    judge_rubric_version  text        NULL,
    rationale             text        NULL,
    unmet_ask             text        NULL,
    evidence_quality      text        NULL,
    judge_failure_reason  text        NULL,
    snapshot_at           timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE memory.operator_turn_legacy_labels IS
    'One row per operator turn whose pre-JC12 label (outcome_id plus the r1/r3 judge detail) was '
    'overwritten by the JC12 projection. Written only by project_turn_outcomes, before its UPDATE. '
    'Full text kept, never truncated (inspection-fidelity.md).';

-- ── 3. The D9 history re-judge run ──────────────────────────────────────────
--
-- One row per re-judge the operator starts (`mike eval rejudge --execute`).
-- The server's judge worker drains the backlog it describes, after live turns,
-- under the JudgeWorker budget share and the run's own daily call cap. Progress
-- and cost are computed at read time from memory.turn_verdicts, with
-- cost = sum(memory.turn_verdicts.cost_usd) over the run's window turns (there is
-- no llm_call_id: the usage sink returns no id; JC12 §12.1 C2), so there are no
-- counters here to drift.
--
-- The eval schema is created by JC06/JC07; IF NOT EXISTS keeps this file
-- order-independent of theirs.

CREATE SCHEMA IF NOT EXISTS eval;

CREATE TABLE IF NOT EXISTS eval.rejudge_runs (
    id               bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    role             text        NOT NULL DEFAULT 'turn_judge'
                                 CHECK (role IN ('turn_judge')),
    bundle_id        bigint      NOT NULL,
    window_since     timestamptz NOT NULL,
    window_until     timestamptz NOT NULL,
    daily_call_cap   integer     NOT NULL CHECK (daily_call_cap >= 1),
    status           text        NOT NULL DEFAULT 'queued'
                                 CHECK (status IN ('queued', 'running', 'done', 'cancelled')),
    estimated_calls  integer     NOT NULL CHECK (estimated_calls >= 0),
    estimated_usd    double precision NULL,
    created_by       text        NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    started_at       timestamptz NULL,
    finished_at      timestamptz NULL,
    CHECK (window_until > window_since)
);

-- bundle_id carries no FK on purpose, matching llm.calls: a recording write
-- must never fail on ordering between this file and JC06's eval.bundles.
-- Only one run may be queued or running at a time.

CREATE UNIQUE INDEX IF NOT EXISTS idx_eval_rejudge_runs_one_active
    ON eval.rejudge_runs ((true))
    WHERE status IN ('queued', 'running');

COMMENT ON TABLE eval.rejudge_runs IS
    'D9: a one-off re-judge of real operator turns in [window_since, window_until) under one live '
    'turn_judge bundle. Drained by the in-server judge worker after live turns; never by the CLI.';
