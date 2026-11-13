ALTER TABLE steering.guard_thresholds DROP CONSTRAINT IF EXISTS guard_thresholds_question_check;
ALTER TABLE steering.guard_thresholds
    ADD CONSTRAINT guard_thresholds_question_check
    CHECK (question IN ('relapse', 'unbacked_wait', 'authorised_handback', 'unobserved_done', 'fire'));

ALTER TABLE steering.guard_thresholds DROP CONSTRAINT IF EXISTS guard_thresholds_label_basis_check;
ALTER TABLE steering.guard_thresholds
    ADD CONSTRAINT guard_thresholds_label_basis_check
    CHECK (label_basis IN ('auto_shadow_gold_relayed', 'holdback_gold_relayed', 'weighted_by_source'));

ALTER TABLE steering.guard_thresholds DROP CONSTRAINT IF EXISTS guard_thresholds_decision_check;
ALTER TABLE steering.guard_thresholds
    ADD CONSTRAINT guard_thresholds_decision_check
    CHECK (decision IN ('applied', 'shadow_only', 'held_hysteresis', 'held_cooldown',
                        'held_wake_floor', 'insufficient', 'below_bar', 'mode_off'));

ALTER TABLE steering.guard_thresholds DROP CONSTRAINT IF EXISTS guard_thresholds_candidate_when_decided;
ALTER TABLE steering.guard_thresholds
    ADD CONSTRAINT guard_thresholds_candidate_when_decided
    CHECK (decision IN ('insufficient', 'below_bar', 'mode_off') OR candidate_threshold IS NOT NULL);

COMMENT ON COLUMN steering.guard_thresholds.question IS
    'One of the turn guard''s four questions, or ''fire'' for a steerer-level threshold on the confidence of every fire of the verifier or monitor-over-sleep steerer (scope global, steerer_id names the steerer).';
COMMENT ON COLUMN steering.guard_thresholds.label_basis IS
    'Which labels counted. weighted_by_source: operator, session_reply, session_end and next_message labels, each source weighted by its guard.threshold.weight.* setting; detail.by_source holds k, n and the weight per source. The two older values record rows written before the sources were weighted: auto_shadow_gold_relayed (a question in shadow) and holdback_gold_relayed (a question in append or wake).';
COMMENT ON COLUMN steering.guard_thresholds.decision IS
    'insufficient: fewer weighted labels than guard.threshold.min_labelled. below_bar: enough labels, but no confidence bin has a Wilson lower bound at or above the precision bar, so the threshold in force is kept. The other values are as in the original table comment.';

CREATE TABLE IF NOT EXISTS steering.threshold_pass_runs (
    rollup_id        bigint PRIMARY KEY,
    first_ran_at     timestamptz NOT NULL DEFAULT now(),
    last_ran_at      timestamptz NOT NULL DEFAULT now(),
    attempts         integer NOT NULL DEFAULT 1 CHECK (attempts >= 1),
    outcome          text NOT NULL
                       CHECK (outcome IN ('wrote', 'no_labels', 'mode_off', 'no_calibration_rows', 'error')),
    threshold_mode   text NOT NULL CHECK (threshold_mode IN ('off', 'shadow', 'apply')),
    rows_written     integer NOT NULL CHECK (rows_written >= 0),
    applied          integer NOT NULL CHECK (applied >= 0),
    n_by_source      jsonb NOT NULL DEFAULT '{}'::jsonb,
    detail           jsonb NOT NULL DEFAULT '{}'::jsonb
);

COMMENT ON TABLE steering.threshold_pass_runs IS
    'One row per eval.rollups generation the threshold pass handled, written whether or not it wrote threshold rows. outcome says why: wrote (rows written with labels counted), no_labels (rows written, every question without a label), mode_off (guard.threshold.mode is off, rows written with decision mode_off), no_calibration_rows (the rollup carries no steer_calibration rows, so there was nothing to calibrate), error (the pass failed; attempts counts the tries and detail.error holds the last message).';
COMMENT ON COLUMN steering.threshold_pass_runs.n_by_source IS
    'Labelled candidates the pass counted, per question (or ''fire:<steerer name>'') and per label source, before weighting.';
