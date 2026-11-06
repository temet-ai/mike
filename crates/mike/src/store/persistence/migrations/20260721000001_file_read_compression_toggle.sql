-- Feature toggle: file_read_compression (session-scoped, default ON —
-- no-default-off rule). Gates the live file-read reducer
-- (crate::compress::file_reads) at the proxy pipeline seam: reduces large
-- `Read` tool_result blocks to a deterministic head/tail excerpt before the
-- request is forwarded upstream.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260713230000_agent_lessons_toggle.sql). Re-add whitelists
-- 'file_read_compression' alongside the existing keys. Idempotent: DROP IF
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
        'file_read_compression'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('file_read_compression', true)
ON CONFLICT (key) DO NOTHING;
