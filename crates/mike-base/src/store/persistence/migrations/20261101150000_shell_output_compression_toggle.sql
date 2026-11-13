-- Feature toggle: shell_output_compression (session-scoped, applies to new
-- sessions, default off). When on, older Bash tool results in history are
-- shortened to their head, tail and error/summary lines with an exact
-- reference to the captured original; shadow mode (the default) records the
-- cut in the manifest and forwards the client's bytes unchanged.
--
-- No toggle row is inserted: an absent row means the code default.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20261101120000_client_serves_git_read_hint_toggle.sql (45 keys); this adds 1.

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
                       'native_peer_mail', 'mod_lesson_delivery',
                       'client_serves_self_identity', 'client_serves_workspace_briefing',
                       'client_serves_change_record_tail', 'client_serves_reminders',
                       'client_serves_recall', 'client_serves_agent_lessons',
                       'client_serves_prompt_shaping', 'client_serves_proxy_tools',
                       'client_serves_weasel_guard', 'client_serves_mail',
                       'context_focus_question', 'client_serves_git_read_hint',
                       'shell_output_compression'));
