-- Feature toggle: channel_link_sweep (global, default ON — no-default-off
-- rule). Gates the periodic background worker
-- (`crate::channel_link_sweep`) that re-derives the Remote Control
-- channel-to-session link from already-captured request bodies, backstopping
-- `proxy::engine::channel_link::maybe_link_channel`'s live, per-request scan:
-- that scan gives up after a bounded number of attempts per session and
-- never re-fires for a session that has since gone idle or a proxy that has
-- since restarted mid-scan.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260829000001_proxy_tools_toggle.sql). Re-add whitelists
-- 'channel_link_sweep' alongside the existing keys. Idempotent: DROP IF
-- EXISTS + ON CONFLICT DO NOTHING.

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
        'channel_link_sweep'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('channel_link_sweep', true)
ON CONFLICT (key) DO NOTHING;
