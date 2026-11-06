--
-- Generalise steering.evaluations from "the verifier's attempts" to "any steerer's attempts, live
-- or shadow, one row per question".
--
-- Why. 20260805400001_steering_evaluations.sql built this table for the verifier alone, and its
-- insert selected the steerer with `WHERE st.kind = 'verifier'`. Its two callers were the turn-close
-- paths (the same-request judge and the quiescence sweep), and both evaluate only steerers whose
-- trigger_point is 'turn_closed', a value steerers_trigger_point_check does not allow. So every one
-- of the 1,255 rows that kept a verdict between 2026-09-22 11:27Z and 2026-09-23 22:00Z says
-- {"judged": false, "abstention": "no_turn_closed_steerer"}, and the verifier's real turn_end
-- evaluations were never recorded at all.
--
-- The columns below let every evaluated steerer record its attempts:
--   mode           live (can deliver) or shadow (never delivers; a would-fire is only recorded).
--                  Held-back events (D8) are JC17's and get their own column there.
--   bundle_id      the eval.bundles id in force (JC06). No FK: a recording write must never fail
--                  because a referenced row is missing (the llm.calls precedent,
--                  20260829000002_llm_calls.sql). JC06 may add the FK once every writer stamps.
--   question       NULL for a single-question steerer; one row per question for the turn guard
--                  (JC16). Extending the set is a new migration.
--   trigger_point  the path that produced the row. NULL on rows written before this migration,
--                  which cannot be attributed.
--   trigger_request_id
--                  the request that triggered the evaluation. No FK (the retention sweep may delete
--                  request rows). The row's operator_turn_id is derived from it by the JC00c mapping
--                  (the request's own operator_turn_id, else the latest operator turn of the session
--                  created at or before it), so the mapping can be recomputed after JC01 changes turn
--                  opening, and the actor (requests.agent_instance_id) is one join away.
--   outcome_label  whether the evaluation's decision was right: with status, right on a
--                  resolved_correction row is a true positive, wrong a false positive; right on a
--                  resolved_pass row a true negative, wrong a false negative.
--   label_source   next_message and session_end are automatic and allowed on shadow rows only:
--                  once a nudge is live and works it prevents the correction that would have
--                  labelled it right (critique defect 4). operator and replay may label any row.
--   labelled_at, label_evidence
--                  when, and from what (the verdict row, eval.labels row and facts used).
--
-- Expand-only. ADD COLUMN with a constant default is metadata-only on PostgreSQL 11+; every CHECK
-- validates against about 3.4k existing rows whose new columns are NULL or 'live'. Idempotent:
-- IF NOT EXISTS on columns and indexes, DROP CONSTRAINT IF EXISTS before each ADD CONSTRAINT.

ALTER TABLE steering.evaluations
    ADD COLUMN IF NOT EXISTS mode           text NOT NULL DEFAULT 'live',
    ADD COLUMN IF NOT EXISTS bundle_id      bigint,
    ADD COLUMN IF NOT EXISTS question       text,
    ADD COLUMN IF NOT EXISTS trigger_point  text,
    ADD COLUMN IF NOT EXISTS trigger_request_id bigint,
    ADD COLUMN IF NOT EXISTS outcome_label  text,
    ADD COLUMN IF NOT EXISTS label_source   text,
    ADD COLUMN IF NOT EXISTS labelled_at    timestamptz,
    ADD COLUMN IF NOT EXISTS label_evidence jsonb;

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_mode_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_mode_check
        CHECK (mode IN ('live', 'shadow'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_question_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_question_check
        CHECK (question IS NULL
               OR question IN ('relapse', 'unbacked_wait', 'authorised_handback', 'unobserved_done'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_trigger_point_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_trigger_point_check
        CHECK (trigger_point IS NULL
               OR trigger_point IN ('user_prompt', 'response', 'turn_end', 'turn_closed', 'quiescence'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_outcome_label_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_outcome_label_check
        CHECK (outcome_label IS NULL OR outcome_label IN ('right', 'wrong', 'indeterminate'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_label_source_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_label_source_check
        CHECK (label_source IS NULL
               OR label_source IN ('next_message', 'session_end', 'operator', 'replay'));

-- A label is all four columns or none of them.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_label_all_or_none;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_label_all_or_none
        CHECK ((outcome_label IS NULL) = (label_source IS NULL)
               AND (label_source IS NULL) = (labelled_at IS NULL)
               AND (labelled_at IS NULL) = (label_evidence IS NULL));

-- Only a resolved evaluation can be right or wrong.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_label_needs_resolution;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_label_needs_resolution
        CHECK (outcome_label IS NULL OR status IN ('resolved_pass', 'resolved_correction'));

-- Critique defect 4 in the schema: automatic labels from the next message or the session end are
-- valid only where nothing was delivered. JC17 replaces this constraint when it adds its holdback
-- arm, whose rows are also undelivered.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_auto_label_shadow_only;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_auto_label_shadow_only
        CHECK (label_source IS NULL
               OR label_source NOT IN ('next_message', 'session_end')
               OR mode = 'shadow');

-- Per steerer and question over time: the stats read (GET /api/steering/evaluation-stats), JC08's
-- daily rollup and JC17's promotion gate.
CREATE INDEX IF NOT EXISTS idx_evaluations_steerer_question_dispatched
    ON steering.evaluations (steerer_id, question, dispatched_at DESC);

-- The labeller's work queue: resolved shadow rows still unlabelled. Stays small because rows leave
-- it as soon as they are labelled.
CREATE INDEX IF NOT EXISTS idx_evaluations_unlabelled_shadow
    ON steering.evaluations (dispatched_at)
    WHERE outcome_label IS NULL AND mode = 'shadow' AND status <> 'dispatched';

COMMENT ON TABLE steering.evaluations IS
    'Every evaluation attempt of any steerer past its invocation gate (dispatched -> resolved_pass / '
    'resolved_correction), live or shadow, one row per question for a multi-question steerer. '
    'steering.interventions still records live fires only; a shadow would-fire has no intervention. '
    'Rows written before JC14 are the verifier''s turn-close attempts and carry trigger_point NULL.';

COMMENT ON COLUMN steering.evaluations.mode IS
    'live: the evaluation could deliver. shadow: it never delivers; a would-fire is recorded as '
    'resolved_correction with intervention_uuid NULL.';

COMMENT ON COLUMN steering.evaluations.bundle_id IS
    'eval.bundles id in force when dispatched (JC06). No FK by design; NULL until JC06 stamps it.';

COMMENT ON COLUMN steering.evaluations.question IS
    'NULL for a single-question steerer; the question slug for the turn guard (JC16).';

COMMENT ON COLUMN steering.evaluations.trigger_point IS
    'The path that produced the row: user_prompt, response, turn_end, turn_closed or quiescence. '
    'NULL on rows written before JC14.';

COMMENT ON COLUMN steering.evaluations.trigger_request_id IS
    'requests.requests id that triggered the evaluation (no FK). operator_turn_id is its JC00c '
    'mapping: the request''s own operator_turn_id, else the latest operator turn of the session '
    'created at or before it. NULL on rows written before JC14.';

COMMENT ON COLUMN steering.evaluations.outcome_label IS
    'right, wrong or indeterminate, read together with status (fire or pass). NULL until labelled.';

COMMENT ON COLUMN steering.evaluations.label_source IS
    'next_message and session_end: automatic, shadow rows only. operator: gold, from the SPA via '
    'eval.labels. replay: an offline replay run.';

COMMENT ON COLUMN steering.evaluations.label_evidence IS
    'Provenance of the label, whole: the verdict or label row it came from and the facts used.';
