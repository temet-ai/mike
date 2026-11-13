-- Phase 23 / LLM-Judge Turn Labeling: schema additions for the judge worker.
--
-- The judge worker runs a periodic sweep over closed, unknown operator turns and
-- calls an OpenRouter-hosted LLM to produce a classification verdict. Two
-- reproducibility metadata columns are added to `memory.operator_turns` so the
-- system can identify which model+rubric produced each label (needed for
-- re-judging when the rubric is updated).
--
-- A partial index on `(closed_at) WHERE outcome_id = 1 AND closed_at IS NOT NULL`
-- backs the claim sweep: only unknown, closed turns are candidates. The turn
-- count is O(all turns), but the candidate set is usually a small tail (the most
-- recently closed turns that have not yet been judged). Without the partial index
-- the claim SELECT would full-scan `memory.operator_turns`; with it the planner
-- uses an index scan over just the unknown-closed subset.
--
-- `config.feature_toggles` carries a closed CHECK constraint
-- (20260615000003_feature_toggles.sql, extended by 20260621000002). The re-add
-- below whitelists BOTH new eval-harness toggle keys ('judge_worker' here and
-- 'session_judge' from the sibling session-judge migration) even though this
-- migration only seeds 'judge_worker'. That keeps the two sibling migrations
-- order-independent: whichever applies second re-adds the identical CHECK, so
-- neither drops a key the other already seeded.
--
-- All DDL is idempotent: IF EXISTS / IF NOT EXISTS / ON CONFLICT DO NOTHING.

-- ─── 1. Reproducibility metadata columns on operator_turns ────────────────────

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS judge_model          text,
    ADD COLUMN IF NOT EXISTS judge_rubric_version text;

-- Both columns are NULL while the turn is unjudged (outcome_id = 1 and no
-- previous judge pass). They are set atomically with outcome_id in
-- `stamp_turn_judgment` so a partial update cannot leave them inconsistent.
COMMENT ON COLUMN memory.operator_turns.judge_model IS
    'OpenRouter model id that produced the judge verdict (e.g. ''openai/gpt-4o''). '
    'NULL until the judge worker has processed this turn. Written by stamp_turn_judgment.';

COMMENT ON COLUMN memory.operator_turns.judge_rubric_version IS
    'Rubric version string used by the judge worker when it labeled this turn '
    '(e.g. ''v1'', ''v2''). Used to re-queue turns when the rubric changes. '
    'NULL until the judge worker has processed this turn.';

-- ─── 2. Partial index for the claim sweep ─────────────────────────────────────

-- Index on closed_at for the WHERE outcome_id = 1 AND closed_at IS NOT NULL
-- subset. ORDER BY closed_at ASC makes the planner scan left-to-right (oldest
-- first) so the batch window is small and cache-hot.
--
-- Cannot run CONCURRENTLY inside a transaction; sqlx runs each migration in its
-- own transaction, so the plain CREATE INDEX form is correct here.
CREATE INDEX IF NOT EXISTS idx_operator_turns_unjudged
    ON memory.operator_turns (closed_at)
    WHERE outcome_id = 1 AND closed_at IS NOT NULL;

-- ─── 3. Extend feature_toggles CHECK constraint (eval-harness toggles) ────────

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync',
        'judge_worker',
        'session_judge'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
-- Only 'judge_worker' is seeded here; the sibling session-judge migration seeds
-- 'session_judge'. The CHECK above already permits both so the two compose.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('judge_worker', true)
ON CONFLICT (key) DO NOTHING;
