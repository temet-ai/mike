-- Promotion of the turn-end guard's questions, the holdback arm, and deferred wakes.
--
-- Why this exists. The guard (JC16) answers four questions at turn end: relapse after a
-- correction, an unbacked wait, an authorised hand-back, and done without observing the
-- result. Each starts in shadow. A question may change traffic only after it has cleared a
-- precision bar on labelled shadow data and the operator has promoted it. Three things then
-- have to be recorded that no table holds today.
--
-- First, a live nudge cannot be scored by the operator's next message, because a nudge that
-- works prevents the correction that would have labelled it right (fleet-mirror critique,
-- defect 4). The design answers this with a random holdback (operator decision D8, 10%):
-- a would-fire event is withheld when a deterministic draw falls below the holdback
-- percentage. Withheld events are the control arm for the causal effect and the only
-- unbiased stream of next-message labels on a live question, so the automatic-label CHECK
-- that JC14 limits to shadow rows is widened to held-back rows, which are just as
-- undelivered. The arm, the draw and the turn the draw was keyed on go on the evaluation.
--
-- Second, a wake must not land on an operator who is still typing (D6: about 30 s; D7: 90 s
-- for waits, at most 3 wakes per session per hour). The delivery spec for #351 forbids a
-- timer per item, so a live wake is written as an intervention that is queued for append at
-- once and is due for a push later; the quiescence worker's existing 60 s tick pushes
-- whatever is due and still undelivered. The due state lives here, so a restart during the
-- grace loses nothing.
--
-- Third, a push reaches few sessions (JC00f: 5-10% of real turns), so whether a wake could
-- have landed is recorded at the due time for both arms (reach_at_due). The holdback
-- comparison then never counts an unreachable fire as a miss.
--
-- steering.question_policies is created by JC16. This file adds the holdback columns, the
-- rule that a hand-back never wakes, and the 'auto' and 'api' writers (automatic demotion,
-- REST calls without an Origin header) to JC16's updated_via CHECK.
--
-- Everything is additive and idempotent: ADD COLUMN IF NOT EXISTS, CREATE ... IF NOT EXISTS,
-- and DROP CONSTRAINT IF EXISTS before every ADD CONSTRAINT. No hash column is indexed or
-- UNIQUE; holdback_salt is data. change_log_id carries no foreign key: recording a promotion
-- must never fail because the referenced row is missing, the reasoning
-- 20260829000002_llm_calls.sql gives for its own ids. No seed: JC16's migration seeds the
-- four policy rows at 'shadow'.

-- ─── 1. Question policies (JC16's table) ─────────────────────────────────────
ALTER TABLE steering.question_policies
    ADD COLUMN IF NOT EXISTS holdback_pct  smallint,
    ADD COLUMN IF NOT EXISTS holdback_salt uuid NOT NULL DEFAULT gen_random_uuid();

ALTER TABLE steering.question_policies
    DROP CONSTRAINT IF EXISTS question_policies_holdback_pct_check;
ALTER TABLE steering.question_policies
    ADD CONSTRAINT question_policies_holdback_pct_check
        CHECK (holdback_pct IS NULL OR (holdback_pct >= 0 AND holdback_pct <= 50));

ALTER TABLE steering.question_policies
    DROP CONSTRAINT IF EXISTS question_policies_handback_never_wakes;
ALTER TABLE steering.question_policies
    ADD CONSTRAINT question_policies_handback_never_wakes
        CHECK (question <> 'authorised_handback' OR mode <> 'wake');

ALTER TABLE steering.question_policies
    DROP CONSTRAINT IF EXISTS question_policies_updated_via_check;
ALTER TABLE steering.question_policies
    ADD CONSTRAINT question_policies_updated_via_check
        CHECK (updated_via IN ('migration', 'spa', 'api', 'cli', 'auto'));

COMMENT ON COLUMN steering.question_policies.holdback_pct IS
    'Per-question holdback percentage, 0-50. NULL uses the setting '
    'steerer.turn_guard.holdback_pct (default 10, operator decision D8).';

COMMENT ON COLUMN steering.question_policies.holdback_salt IS
    'Salt of the deterministic arm draw: sha256(salt:steerer_id:question:anchor_turn_id). '
    'Fixed so every assignment can be recomputed; changing it is a logged act.';

COMMENT ON COLUMN steering.question_policies.updated_via IS
    'Who last wrote the mode: migration, spa, api, cli (operator acts) or auto (JC17 automatic '
    'demotion). Every change to or from append or wake also has a steering.question_promotions row.';

-- ─── 2. Promotion history ────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS steering.question_promotions (
    id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    policy_id            bigint NOT NULL
                           REFERENCES steering.question_policies(id) ON DELETE CASCADE,
    from_mode            text NOT NULL
                           CHECK (from_mode IN ('off', 'shadow', 'append', 'wake')),
    to_mode              text NOT NULL
                           CHECK (to_mode IN ('off', 'shadow', 'append', 'wake')),
    action               text NOT NULL
                           CHECK (action IN ('promote', 'demote', 'auto_demote')),
    decided_via          text NOT NULL
                           CHECK (decided_via IN ('spa', 'api', 'cli', 'auto')),
    decided_at           timestamptz NOT NULL DEFAULT now(),
    gate_passed          boolean NOT NULL,
    gate_json            jsonb NOT NULL,
    override_reason      text,
    replaces_steerer_id  bigint
                           REFERENCES steering.steerers(id) ON DELETE SET NULL,
    restored_steerer_id  bigint
                           REFERENCES steering.steerers(id) ON DELETE SET NULL,
    change_log_id        bigint,
    CONSTRAINT question_promotions_override_check
        CHECK (gate_passed OR action <> 'promote' OR override_reason IS NOT NULL),
    CONSTRAINT question_promotions_auto_check
        CHECK ((action = 'auto_demote') = (decided_via = 'auto'))
);

CREATE INDEX IF NOT EXISTS idx_question_promotions_policy_decided
    ON steering.question_promotions (policy_id, decided_at DESC);
CREATE INDEX IF NOT EXISTS idx_question_promotions_replaces
    ON steering.question_promotions (replaces_steerer_id)
    WHERE replaces_steerer_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_question_promotions_restored
    ON steering.question_promotions (restored_steerer_id)
    WHERE restored_steerer_id IS NOT NULL;

COMMENT ON TABLE steering.question_promotions IS
    'Append-only history of every mode change of a guard question to or from append or wake: '
    'who or what decided it, the full gate evidence at that moment, and an override reason when '
    'a promotion was made against a failing gate.';

COMMENT ON COLUMN steering.question_promotions.gate_json IS
    'The whole gate evidence at decision time (counts, precision, Wilson bounds, bar, '
    'preconditions, reasons; for unobserved_done the verifier comparison). Never a summary.';

COMMENT ON COLUMN steering.question_promotions.change_log_id IS
    'The config.change_log row (JC05 append_change, subject_kind promotion) written in the same '
    'transaction. No foreign key: recording a promotion must not fail on a missing change-log row.';

-- ─── 3. Evaluation arms and reach (JC14's table) ─────────────────────────────
-- Treated and held-back rows are live evaluations (mode = 'live'); shadow rows carry no arm.
ALTER TABLE steering.evaluations
    ADD COLUMN IF NOT EXISTS arm                text,
    ADD COLUMN IF NOT EXISTS holdback_draw      double precision,
    ADD COLUMN IF NOT EXISTS arm_anchor_turn_id bigint,
    ADD COLUMN IF NOT EXISTS due_at             timestamptz,
    ADD COLUMN IF NOT EXISTS reach_at_due       text;

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_arm_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_arm_check
        CHECK (arm IS NULL OR arm IN ('treated', 'holdback'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_holdback_draw_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_holdback_draw_check
        CHECK (holdback_draw IS NULL OR (holdback_draw >= 0 AND holdback_draw < 1));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_reach_at_due_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_reach_at_due_check
        CHECK (reach_at_due IS NULL OR reach_at_due IN ('reachable', 'not_idle', 'no_stream'));

ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_arm_complete_check;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_arm_complete_check
        CHECK (arm IS NULL
               OR (holdback_draw IS NOT NULL AND arm_anchor_turn_id IS NOT NULL AND mode = 'live'));

-- Replaces JC14's constraint of the same name. Automatic labels (next message, session end)
-- stay invalid on any delivered row; a held-back row was never delivered, so it may carry one.
ALTER TABLE steering.evaluations DROP CONSTRAINT IF EXISTS evaluations_auto_label_shadow_only;
ALTER TABLE steering.evaluations
    ADD CONSTRAINT evaluations_auto_label_shadow_only
        CHECK (label_source IS NULL
               OR label_source NOT IN ('next_message', 'session_end')
               OR mode = 'shadow'
               OR arm IS NOT DISTINCT FROM 'holdback');

CREATE INDEX IF NOT EXISTS idx_evaluations_arm
    ON steering.evaluations (steerer_id, question, dispatched_at)
    WHERE arm IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_evaluations_arm_anchor
    ON steering.evaluations (arm_anchor_turn_id)
    WHERE arm IS NOT NULL;
-- The labeller's second work queue beside JC14's idx_evaluations_unlabelled_shadow.
CREATE INDEX IF NOT EXISTS idx_evaluations_unlabelled_holdback
    ON steering.evaluations (dispatched_at)
    WHERE outcome_label IS NULL AND arm = 'holdback' AND status <> 'dispatched';
-- The due pass's reach check for both arms.
CREATE INDEX IF NOT EXISTS idx_evaluations_reach_due
    ON steering.evaluations (due_at)
    WHERE due_at IS NOT NULL AND reach_at_due IS NULL;

COMMENT ON COLUMN steering.evaluations.arm IS
    'For a would-fire of a live (append or wake) question: treated (delivered through an '
    'intervention) or holdback (withheld at random, no intervention). Both are mode live. NULL '
    'on shadow rows and on steerers without a holdback.';

COMMENT ON COLUMN steering.evaluations.arm_anchor_turn_id IS
    'The operator turn the arm draw was keyed on: the correcting turn for relapse, the '
    'evaluated turn otherwise. Re-evaluating the same event gives the same arm.';

COMMENT ON COLUMN steering.evaluations.due_at IS
    'For a would-fire of a question in wake mode, in either arm: turn end plus the question''s '
    'grace, when a push would be attempted. NULL for append-mode questions.';

COMMENT ON COLUMN steering.evaluations.reach_at_due IS
    'Recorded by the due pass at due_at for both arms: reachable (the session was idle and had '
    'an open Remote Control stream), not_idle (the operator or the session had moved on) or '
    'no_stream (idle, but no stream to push on).';

-- ─── 4. Deferred delivery (steering.interventions) ───────────────────────────
ALTER TABLE steering.interventions
    ADD COLUMN IF NOT EXISTS delivery_channel  text,
    ADD COLUMN IF NOT EXISTS deliver_after     timestamptz,
    ADD COLUMN IF NOT EXISTS wake_state        text,
    ADD COLUMN IF NOT EXISTS wake_claimed_at   timestamptz,
    ADD COLUMN IF NOT EXISTS wake_resolved_at  timestamptz,
    ADD COLUMN IF NOT EXISTS wake_attempts     smallint NOT NULL DEFAULT 0;

ALTER TABLE steering.interventions DROP CONSTRAINT IF EXISTS interventions_delivery_channel_check;
ALTER TABLE steering.interventions
    ADD CONSTRAINT interventions_delivery_channel_check
        CHECK (delivery_channel IS NULL OR delivery_channel IN ('append', 'wake'));

ALTER TABLE steering.interventions DROP CONSTRAINT IF EXISTS interventions_wake_state_check;
ALTER TABLE steering.interventions
    ADD CONSTRAINT interventions_wake_state_check
        CHECK (wake_state IS NULL
               OR wake_state IN ('pending', 'delivering', 'pushed', 'push_failed', 'unreachable',
                                 'appended', 'reverted', 'backed', 'capped_hour',
                                 'capped_correction', 'expired', 'disabled',
                                 'operator_rejected', 'abandoned'));

ALTER TABLE steering.interventions DROP CONSTRAINT IF EXISTS interventions_deferred_has_due_check;
ALTER TABLE steering.interventions
    ADD CONSTRAINT interventions_deferred_has_due_check
        CHECK (wake_state IS NULL
               OR wake_state NOT IN ('pending', 'delivering')
               OR (delivery_channel = 'wake' AND deliver_after IS NOT NULL));

CREATE INDEX IF NOT EXISTS idx_interventions_wake_due
    ON steering.interventions (deliver_after)
    WHERE wake_state = 'pending';
CREATE INDEX IF NOT EXISTS idx_interventions_wake_delivering
    ON steering.interventions (wake_claimed_at)
    WHERE wake_state = 'delivering';

COMMENT ON COLUMN steering.interventions.delivery_channel IS
    'How a guard fire is delivered: append (queued for the next request only) or wake (queued '
    'for the next request at once, and pushed at deliver_after if still undelivered and the '
    'session is idle). NULL for legacy steerers, which deliver at fire time as before.';

COMMENT ON COLUMN steering.interventions.deliver_after IS
    'When a wake becomes due for a push: turn end plus the question''s grace. The quiescence '
    'worker''s 60 s tick handles due rows; nothing is scheduled per row.';

COMMENT ON COLUMN steering.interventions.wake_state IS
    'Push decision of a wake: pending, delivering (claimed by a tick), then pushed, or why no '
    'push was made (appended or reverted before the due time, push_failed, unreachable, backed, '
    'capped_hour, expired, disabled, operator_rejected, abandoned), or capped_correction for a '
    'relapse fire beyond the per-correction limit. The append outcome is delivered_request_id or '
    'revert_reason, as for every steerer.';

-- ─── 5. Alarm kinds (JC08's table) ───────────────────────────────────────────
-- The list is the one the semantic-context migration left (JC08's kinds, the kinds other
-- packages reserved, JC17's four steer_* kinds and embedding_drift). Never drop a kind.
ALTER TABLE eval.alarms DROP CONSTRAINT IF EXISTS alarms_kind_check;
ALTER TABLE eval.alarms
    ADD CONSTRAINT alarms_kind_check
        CHECK (kind IN ('label_mix_shift', 'unjudged_backlog', 'judge_stalled',
                        'budget_refusals', 'change_without_gate', 'gold_label_by_agent',
                        'steer_question_eligible', 'steer_question_demoted',
                        'steer_holdback_harm', 'steer_wrong_wake_ceiling',
                        'metric_drop', 'shadow_disagreement',
                        'judge_call_loop',
                        'embedding_drift'));
