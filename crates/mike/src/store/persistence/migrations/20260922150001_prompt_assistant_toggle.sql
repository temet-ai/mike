-- Feature toggle prompt_assistant, default ON.
--
-- Unlike the background LLM callers (steerer, judge_worker, session_judge)
-- that this repo's default-off-mutation-and-egress rule keeps off by
-- default, the prompt assistant never runs on its own: it is an
-- operator-invoked chat, like Claude Code itself, that only calls out when
-- the operator sends it a message about a prompt they are looking at, and
-- its may_apply setting defaults to 'suggest' so it never writes a filter
-- rule without the operator applying it. The toggle exists to let an
-- operator hide the panel entirely, not to gate an autonomous scanner.
-- operator_set stays false so the code default (true) wins until an
-- operator flips it, same as 20260921000003_workspace_briefing_toggle.sql.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260921000003_workspace_briefing_toggle.sql). Re-add whitelists the new
-- key alongside every existing one. Idempotent: DROP IF EXISTS + ON
-- CONFLICT DO NOTHING.

ALTER TABLE config.feature_toggles
    DROP CONSTRAINT IF EXISTS feature_toggles_key_check;

ALTER TABLE config.feature_toggles
    ADD CONSTRAINT feature_toggles_key_check
    CHECK (key IN (
        'reminders',
        'recall_injector',
        'weasel_intercept',
        'steerer',
        'lesson_worker',
        'harvester',
        'db_resync',
        'judge_worker',
        'session_judge',
        'reminder_on_token',
        'mailbox_redelivery',
        'agent_lessons',
        'file_read_compression',
        'retention_sweep',
        'proxy_tools',
        'channel_link_sweep',
        'workspace_capture_registration',
        'self_identity_reminder',
        'workspace_briefing',
        'prompt_assistant'
    ));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('prompt_assistant', true, false)
ON CONFLICT (key) DO NOTHING;
