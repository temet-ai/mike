-- JC12 section 12.4's promotion alarms: shadow_disagreement (subject
-- turn_judge:<live bundle id>) and metric_drop (subjects turn_judge:<live bundle
-- id>:precision and :recall). Both kinds were reserved by JC08's original CHECK
-- (20260924230000_eval_rates_alarms.sql) and have already been carried forward
-- by every migration since that re-creates the CHECK, so this statement is a
-- defensive no-op today. It exists so a re-created CHECK that drops one of them
-- by mistake is caught here rather than downstream, and so this list, not an
-- earlier one, is the last word on what JC12 shipped.

ALTER TABLE eval.alarms DROP CONSTRAINT IF EXISTS alarms_kind_check;
ALTER TABLE eval.alarms
    ADD CONSTRAINT alarms_kind_check
        CHECK (kind IN ('label_mix_shift', 'unjudged_backlog', 'judge_stalled',
                        'budget_refusals', 'change_without_gate', 'gold_label_by_agent',
                        'steer_question_eligible', 'steer_question_demoted',
                        'steer_holdback_harm', 'steer_wrong_wake_ceiling',
                        'metric_drop', 'shadow_disagreement',
                        'judge_call_loop',
                        'embedding_drift',
                        'label_band'));
