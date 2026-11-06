-- Feature toggle: repeated_notices (session, default OFF). Claude Code adds a
-- standing notice to every message another session pushes to a session and to
-- every channel message that arrives mid-turn. When this toggle is on, mike
-- keeps the first copy of each notice in a conversation and removes the later
-- copies from the request it forwards. The captured request is unchanged.
--
-- Default OFF: it changes the forwarded request body.
--
-- No row is inserted: an absent row means the code default, and a default
-- never rewrites a stored value.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20260930000100_fleet_auto_start_toggle.sql (28 keys). Idempotent: DROP IF
-- EXISTS + re-ADD with every existing key plus 'repeated_notices'.

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
                       'session_end_shadow', 'launch_brief', 'judge_lessons', 'fleet_auto_start',
                       'repeated_notices'));
