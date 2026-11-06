-- Feature toggle: retention_sweep (global, default ON — no-default-off
-- rule). Gates the periodic background worker that calls
-- `sweep_expired_sessions` on a timer and batches the companion
-- `messages.messages` orphan sweep — GitHub issue #103. Distinct from
-- `sweep_at_startup` (recorded operator-approval carve-out, stays
-- default-off, untouched by this migration): this toggle controls the
-- new recurring loop, not the one-shot boot-time sweep.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260721000001_file_read_compression_toggle.sql). Re-add whitelists
-- 'retention_sweep' alongside the existing keys. Idempotent: DROP IF
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
        'retention_sweep'
    ));

-- Seed the new toggle enabled=true (no-default-off rule). Idempotent.
INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('retention_sweep', true)
ON CONFLICT (key) DO NOTHING;
