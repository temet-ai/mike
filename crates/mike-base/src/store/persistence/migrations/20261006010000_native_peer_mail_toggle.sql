-- Feature toggle: native_peer_mail (global, default OFF). When on, mike-mcp
-- delivers a session's mail by posting it to that session's own Claude Code
-- inbox socket as a native cross-session message, instead of as a mike channel
-- message.
--
-- No row is inserted: an absent row means the code default, and a default
-- never rewrites a stored value.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20261005030000_change_record.sql (31 keys). Idempotent: DROP IF EXISTS +
-- re-ADD with every existing key plus 'native_peer_mail'.

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
                       'repeated_notices', 'change_record', 'change_record_tail',
                       'native_peer_mail'));
