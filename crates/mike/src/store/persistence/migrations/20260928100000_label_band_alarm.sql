-- JC08 section 4.4.1's band detector ('band:turn_judge/corrected') ships with the rest of
-- JC08 but its kind was left out of eval.alarms' CHECK: the band asks whether the stored
-- corrected share on replied real turns is even plausible against the operator's measured
-- 32-49% weekly correction rate, independent of the shift detector's label_mix_shift kind.
-- It is gated off by eval.alarm.label_band.enabled (default false) until a promoted judge
-- backs the reaction axis, so adding the kind here raises nothing by itself.

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
