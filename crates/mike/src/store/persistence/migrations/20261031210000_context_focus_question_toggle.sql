-- Feature toggle: context_focus_question (session scope, applies to new
-- sessions, default off). When on, the proxy asks the model for one line per
-- tool call saying what it wants from the call, records it with the call, and
-- strips it before the client sees the tool_use (mike#614 shape 1).
--
-- No toggle row is inserted: an absent row means the code default.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20261028090000_client_serves_toggles.sql (43 keys); this adds 1.

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
                       'context_focus_question'));
