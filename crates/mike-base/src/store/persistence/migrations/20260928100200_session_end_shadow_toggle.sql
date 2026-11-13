-- JC13 gap fill: the session-end shadow path's feature toggle.
--
-- Why. 20260925000500_session_end_classification.sql shipped session_judges.verdicts.mode
-- ('live', 'shadow', 'eval') and mike.session_quality_v already reads mode = 'live' only, but that
-- migration deliberately left the shadow worker itself unbuilt ("The session-end shadow path, its
-- bundle role and its toggle are not built (a pre-promotion soak, removed from JC13)"). The session
-- judge is live on bundle 13 now (promoted by override), so a future candidate bundle needs a way
-- to be judged in shadow, on a sampled share of closed sessions, without ever touching
-- operator_turns.outcome_id: JC13's own projection branch (§4.6) reads session_judges.verdicts with
-- `mode = 'live'` only, so a shadow row is structurally invisible to projection.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced 20260615000003, last
-- widened by 20260925000802_directive_proposals.sql, 24 keys). This migration widens it to add
-- 'session_end_shadow' alongside every existing key. Idempotent: DROP IF EXISTS + re-ADD. No row is
-- inserted for the new key: an absent row means the code default, which is off (this toggle gates
-- content egress to OpenRouter under a candidate configuration, so default-off-mutation-and-egress.md
-- requires it off), and a default never rewrites a stored value.
--
-- Expand-only and idempotent.

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;
ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
        CHECK (key IN ('reminders', 'recall_injector', 'weasel_intercept', 'steerer', 'lesson_worker',
                       'harvester', 'db_resync', 'judge_worker', 'session_judge', 'reminder_on_token',
                       'mailbox_redelivery', 'agent_lessons', 'file_read_compression', 'retention_sweep',
                       'proxy_tools', 'channel_link_sweep', 'workspace_capture_registration',
                       'self_identity_reminder', 'workspace_briefing', 'prompt_assistant',
                       'dossier_builder', 'semantic_context', 'turn_guard', 'directive_proposer',
                       'session_end_shadow'));
