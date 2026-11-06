-- Feature toggle: workspace_briefing (session-scoped, default OFF). Gates the
-- proxy-side stage that injects the workspace briefing (session id, project,
-- worktree, roster peers, how to reach them) into a new session's first
-- main-agent request. Replaces the retired SessionStart hook path.
--
-- Default OFF per the default-off rule for mutation of the outbound wire.
-- operator_set stays false so the code default wins until an operator flips it.

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
        'workspace_briefing'
    ));

INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES
    ('workspace_briefing', false, false)
ON CONFLICT (key) DO NOTHING;
