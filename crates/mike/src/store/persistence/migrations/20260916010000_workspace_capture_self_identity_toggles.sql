-- Feature toggles: workspace_capture_registration and self_identity_reminder
-- (both session-scoped, default ON per the no-default-off rule).
--
-- Every `/v1/messages` request already carries this session's id and, once
-- resolved, its canonical project path (`RequestSignals`) — data the two
-- SessionStart hooks (`workspace-register.sh`, `inject-session-id.sh`)
-- otherwise depend on the harness actually firing the hook to deliver.
-- These toggles gate the proxy's own best-effort backstops for that data:
--
--   * workspace_capture_registration — registers a session on the
--     workspace roster from its first captured request when the hook never
--     reached `/api/workspace/register` (`registered_via = 'passive'`, the
--     existing "back-filled from traffic" vocabulary value — no new
--     `registered_via` CHECK value needed).
--   * self_identity_reminder — appends a `<system-reminder>` telling the
--     model its own `session_id`, mirroring `inject-session-id.sh`'s
--     `additionalContext` block, once per session per process.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260916000002_channel_link_sweep_toggle.sql). Re-add whitelists both
-- new keys alongside every existing one. Idempotent: DROP IF EXISTS + ON
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
        'self_identity_reminder'
    ));

INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('workspace_capture_registration', true),
    ('self_identity_reminder',         true)
ON CONFLICT (key) DO NOTHING;
