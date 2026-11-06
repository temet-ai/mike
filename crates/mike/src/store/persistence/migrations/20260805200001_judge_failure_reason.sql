-- ════════════════════════════════════════════════════════════════════
-- memory.operator_turns.judge_failure_reason — distinguish a failed judge
-- call from "never judged" and "judged, inconclusive" (issue #124 AC2)
-- ════════════════════════════════════════════════════════════════════
--
-- Before this column, every turn with outcome_id = 1 ('unknown') was
-- indistinguishable across three genuinely different states:
--   1. never judged            — judge_rubric_version IS NULL
--   2. judged, inconclusive    — judge_rubric_version IS NOT NULL (the LLM
--                                 answered but confidence was below threshold,
--                                 so judge_worker::run_judge_sweep stamps
--                                 'unknown' deliberately)
--   3. judge call failed       — run_judge exhausted its retry budget (#124
--                                 core slice) or hit a permanent error; the
--                                 row was simply left untouched
-- States 1 and 3 were identical on disk (both NULL judge_rubric_version),
-- which is exactly the #124 bug: a burst of OpenRouter 429s during
-- session_judge's 25-session sweep silently biased any rate computed over
-- `outcome` by an unknown amount, indistinguishable from turns that were
-- simply never reached yet.
--
-- judge_failure_reason distinguishes state 3 from the other two. It is
-- cleared back to NULL by stamp_turn_judgment whenever a later sweep
-- succeeds (claim_unjudged_turns already re-claims judge_rubric_version IS
-- NULL rows regardless of this column, so a failed turn keeps retrying on
-- schedule — this column only makes WHY it is still unknown legible).
--
-- Three-state query (AC2 evidence):
--   SELECT
--       count(*) FILTER (WHERE outcome_id = 1 AND judge_rubric_version IS NULL
--                          AND judge_failure_reason IS NULL)      AS never_judged,
--       count(*) FILTER (WHERE outcome_id = 1 AND judge_rubric_version IS NOT NULL)
--                                                                  AS judged_inconclusive,
--       count(*) FILTER (WHERE judge_failure_reason IS NOT NULL)  AS judge_failed
--     FROM memory.operator_turns;
--
-- AC4 (historical damage): the two pre-existing states (never_judged,
-- judge_failed) are BOTH represented as judge_rubric_version IS NULL today
-- and are NOT separable retroactively — there is no signal left on disk
-- (no error log join key, no failed-attempt timestamp) that distinguishes
-- "never claimed yet" from "claimed and failed" for a historical row.
-- Documented as UNRECOVERABLE rather than silently inherited. The
-- conflated total as of this migration (run once, for the record):
--   SELECT count(*) FROM memory.operator_turns
--    WHERE outcome_id = 1 AND judge_rubric_version IS NULL;
-- Going forward, every failure this column can see is separable from
-- "never judged" from the moment a sweep first attempts it.

ALTER TABLE memory.operator_turns
    ADD COLUMN IF NOT EXISTS judge_failure_reason text
        CHECK (judge_failure_reason IN ('transient_exhausted', 'permanent'));

COMMENT ON COLUMN memory.operator_turns.judge_failure_reason IS
    'Set only when a judge call for this turn failed (transient retries '
    'exhausted, or a permanent error) — NULL for a turn never yet judged '
    'or one judged with an inconclusive (below-threshold) verdict. '
    'Cleared back to NULL by stamp_turn_judgment on a later successful '
    'sweep. See migration comment for the three-state disambiguation '
    'query (issue #124 AC2).';

-- ─── steering.false_done_rate_v: exclude judge-failed fires (AC5) ─────────
--
-- fires_count was every fire regardless of whether its turn was ever
-- actually judged; a fire whose turn's judge call failed counted toward
-- the denominator but could never count toward corrected_count (its
-- outcome stays 'unknown' forever unless a later sweep succeeds), silently
-- deflating corrected_rate by exactly the judge-failure rate. Excluding
-- those fires from both counts keeps the rate honest; reporting the
-- excluded count keeps the exclusion visible rather than a silent drop
-- (inspection-fidelity.md).

-- `CREATE OR REPLACE VIEW` cannot reorder or rename existing output columns
-- (Postgres error: "cannot change name of view column ... to ...") — the new
-- excluded_judge_failed_count column is appended LAST, after the existing
-- five, so this replace is structurally additive.
CREATE OR REPLACE VIEW steering.false_done_rate_v AS
SELECT
    p.project_path,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)              AS fires_count,
    count(i.id) FILTER (WHERE o.code = 'corrected')                        AS corrected_count,
    count(i.id) FILTER (
        WHERE i.delivered_request_id IS NOT NULL
          AND ot.judge_failure_reason IS NULL
    )                                                                       AS delivered_count,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (WHERE o.code = 'corrected')::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS corrected_rate,
    CASE WHEN count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL) = 0
         THEN NULL::double precision
         ELSE round(
             count(i.id) FILTER (
                 WHERE i.delivered_request_id IS NOT NULL
                   AND ot.judge_failure_reason IS NULL
             )::numeric
             / count(i.id) FILTER (WHERE ot.judge_failure_reason IS NULL)::numeric, 4)::double precision
    END AS delivered_rate,
    count(i.id) FILTER (WHERE ot.judge_failure_reason IS NOT NULL)          AS excluded_judge_failed_count
FROM steering.interventions i
JOIN sessions.sessions s ON s.id = i.session_id
JOIN projects.projects p ON p.id = s.project_id
LEFT JOIN (
    SELECT operator_turns.session_id,
           operator_turns.start_request_id,
           operator_turns.end_request_id,
           operator_turns.outcome_id,
           operator_turns.judge_failure_reason
      FROM memory.operator_turns
    OFFSET 0
) ot ON ot.session_id = i.session_id
       AND ot.start_request_id <= i.trigger_request_id
       AND ot.end_request_id >= i.trigger_request_id
LEFT JOIN memory.turn_outcomes o ON o.id = ot.outcome_id
WHERE i.steerer_id IN (
    SELECT steerers.id FROM steering.steerers WHERE steerers.kind = 'false_done'
)
GROUP BY p.project_path;
