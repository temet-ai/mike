-- Feature toggle: launch_brief (session-scoped, default OFF). Gates the
-- proxy's launch_brief stage, which attaches a spawned session's launch brief
-- (launch.launches.brief_text for a brief_mode = 'note' launch) to every
-- request of that session. `mike spawn open` and `spawn_session` read the
-- same toggle: while it is off they keep staging long briefs to a file, so a
-- note-mode pointer is never typed into a session nothing will deliver to.
--
-- Default OFF per default-off-mutation-and-egress.md: the stage adds to the
-- outbound request. No row is inserted: an absent row means the code default,
-- and a default never rewrites a stored value.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20260928100200_session_end_shadow_toggle.sql (25 keys). Idempotent:
-- DROP IF EXISTS + re-ADD with every existing key plus 'launch_brief'.

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
                       'session_end_shadow', 'launch_brief'));
