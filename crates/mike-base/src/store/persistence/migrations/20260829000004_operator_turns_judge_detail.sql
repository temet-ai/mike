-- artifacts/2026-08-29/llm-cost-and-signal-plan.md, "Making the judge
-- output worth reading".
--
-- The turn judge (crates/mike/src/judge_worker.rs's run_judge_sweep, via
-- stamp_turn_judgment) currently writes only a label
-- (memory.operator_turns.outcome_id) and reproducibility metadata
-- (judge_model, judge_rubric_version — 20260625000001_judge_labeling.sql).
-- Nothing records WHY: no citation, no statement of what the operator
-- asked for and did not get, no signal distinguishing a turn that proved
-- its work from one that only claimed to. This is that row: operator_turns
-- is where every other piece of the turn judge's verdict already lives,
-- and session_judges.verdicts is a DIFFERENT judge (the per-session
-- quality worker, GitHub issue #24) that already has its own rationale
-- and quality_score — adding these columns there would answer a question
-- about the wrong judge.
--
-- All three nullable, no default: rows judged under the prior rubric
-- version (r1-2026-06-22) never had these fields to report and stay NULL
-- until re-judged under the bumped JUDGE_RUBRIC_VERSION — the existing
-- `judge_rubric_version <> $1` predicate in claim_unjudged_turns already
-- re-queues them for that.

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS rationale        text,
    ADD COLUMN IF NOT EXISTS unmet_ask        text,
    ADD COLUMN IF NOT EXISTS evidence_quality text
                              CHECK (evidence_quality IN ('verified', 'asserted', 'absent'));

COMMENT ON COLUMN memory.operator_turns.rationale IS
    'One or two sentences from the turn judge citing what in the turn '
    'supports its outcome_id label. NULL until judged under the rubric '
    'version that introduced this field.';

COMMENT ON COLUMN memory.operator_turns.unmet_ask IS
    'What the operator asked for that this turn did not deliver, per the '
    'turn judge. Empty string when nothing was unmet; NULL until judged '
    'under the rubric version that introduced this field.';

COMMENT ON COLUMN memory.operator_turns.evidence_quality IS
    'verified = the turn showed the output of a check it ran; asserted = '
    'it claimed a result without showing one; absent = it claimed nothing. '
    'The field that distinguishes an outcome_id of accepted backed by '
    'proof from one that only says so. NULL until judged under the rubric '
    'version that introduced this field.';
