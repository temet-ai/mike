-- ════════════════════════════════════════════════════════════════════
-- memory.operator_turns judge retry bookkeeping (issue #151)
-- ════════════════════════════════════════════════════════════════════
--
-- claim_unjudged_turns had no attempt counter, no failure timestamp, and
-- no backoff column in its predicate — a permanently-failing turn (a 402
-- from an exhausted OpenRouter key, in the incident that opened this
-- issue) requalified on every sweep forever, and the worker never
-- consulted judge_failure_reason (already written by mark_judge_failed,
-- #124 AC2) when deciding what to reclaim. One proxy process made 54,130
-- rejected LLM calls against ~80 turns in 14 hours as a result.
--
-- judge_attempt_count / judge_next_attempt_at give claim_unjudged_turns
-- what it was missing:
--   - judge_failure_reason = 'permanent' is now excluded from the claim
--     predicate outright — a permanent failure is a terminal state,
--     inspectable via this column, cleared only by a deliberate operator
--     UPDATE (there is no automatic path back onto the queue).
--   - judge_next_attempt_at holds a per-item exponential-backoff deadline
--     for a 'transient_exhausted' failure, computed by mark_judge_failed
--     in the same UPDATE that records the failure (doubling per attempt,
--     capped at steerer.judge.backoff_max_secs) — no more flat retry
--     every tick.
--
-- Both columns are reset by stamp_turn_judgment on a successful sweep so
-- a later rubric-version requeue does not inherit a stale attempt count
-- or backoff deadline.

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS judge_attempt_count   integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS judge_next_attempt_at  timestamptz;

COMMENT ON COLUMN memory.operator_turns.judge_attempt_count IS
    'Count of failed judge attempts since the turn was last (never, or '
    'successfully) judged. Incremented by mark_judge_failed, reset to 0 '
    'by stamp_turn_judgment on success. Feeds the exponential backoff '
    'written to judge_next_attempt_at.';

COMMENT ON COLUMN memory.operator_turns.judge_next_attempt_at IS
    'Earliest time claim_unjudged_turns may reclaim this turn after a '
    'transient_exhausted failure. NULL for a turn never failed, a turn '
    'judged successfully, or a turn whose most recent failure was '
    'permanent (permanent failures are excluded from the claim predicate '
    'entirely, not backed off).';
