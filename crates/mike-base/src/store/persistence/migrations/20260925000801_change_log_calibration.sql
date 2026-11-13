-- JC18 writes two kinds of change the JC05 draft cannot hold
-- (plans/judgement-context/migrations/JC05_config_change_log.sql:34-43):
--   * subject_kind 'guard_threshold': an applied calibrated threshold, written
--     by the threshold pass in the same statement as its
--     steering.guard_thresholds row, with via 'calibration' and
--     detail.threshold_row_id;
--   * subject_kind 'directive_decision': an operator's approve or reject of a
--     memory.directive_proposals row, written with JC05's append_change inside
--     the decision's transaction.
--
-- BUILD-TIME STEP: both CHECKs are re-created in full. Regenerate each list
-- from main's current constraint (pg_get_constraintdef on
-- pg_constraint WHERE conrelid = 'config.change_log'::regclass) and append the
-- new values. The lists below are JC05's draft plus JC18's values and will be
-- stale if any other package widened them in between. The constraint names are
-- PostgreSQL's defaults for JC05's inline CHECKs; confirm them the same way.
--
-- Also confirm that config_snapshot_at (JC05 §4.2 step 8) does not return the
-- two new kinds as settings: JC18 calls it with kind = 'setting'.
--
-- Expand-only: widening a CHECK re-validates about 70 baseline rows plus a few
-- a day.

SET LOCAL lock_timeout = '5s';

ALTER TABLE config.change_log DROP CONSTRAINT IF EXISTS change_log_subject_kind_check;
ALTER TABLE config.change_log
    ADD CONSTRAINT change_log_subject_kind_check
    CHECK (subject_kind IN ('setting', 'toggle', 'steerer', 'build', 'promotion',
                            'guard_threshold', 'directive_decision'));

ALTER TABLE config.change_log DROP CONSTRAINT IF EXISTS change_log_via_check;
ALTER TABLE config.change_log
    ADD CONSTRAINT change_log_via_check
    CHECK (via IN ('spa', 'api', 'mcp', 'cli', 'startup', 'migration',
                   'promotion', 'mike', 'sql', 'calibration'));


ALTER TABLE eval.rates DROP CONSTRAINT IF EXISTS eval_rates_family_metric;
ALTER TABLE eval.rates
    ADD CONSTRAINT eval_rates_family_metric CHECK (
        (family = 'accuracy'          AND metric IN ('precision', 'recall')) OR
        (family IN ('label_mix', 'base_rate') AND metric = 'share') OR
        (family = 'steer_calibration' AND metric IN ('precision', 'fire_rate', 'fire_precision', 'fire_recall')));
