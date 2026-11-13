-- Feature toggles: client_serves_* (global, applies immediately). Each lets a
-- client such as tether take one proxy stage over for a session through that
-- stage's mike route; while one is off its route takes nothing over and the
-- proxy keeps the stage. client_serves_self_identity defaults on (tether#1
-- measured step 1 reading 99.7% of its prompt from cache with tether placing
-- the text, 43.6% with mike splicing it); the others default off and are moved
-- one at a time. docs/ARCHITECTURE.md section 4.7 has the per-stage table.
--
-- No toggle row is inserted: an absent row means the code default.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20261014180000_mod_lesson_delivery.sql (33 keys); this adds 10.

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
                       'client_serves_weasel_guard', 'client_serves_mail'));
