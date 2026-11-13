-- Feature toggle: reminder_on_token (session-scoped, default ON).
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260625000002_session_judge.sql). Re-add whitelists 'reminder_on_token'
-- alongside the existing keys. Idempotent: DROP IF EXISTS + ON CONFLICT DO
-- NOTHING.

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
        'reminder_on_token'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('reminder_on_token', true)
ON CONFLICT (key) DO NOTHING;
