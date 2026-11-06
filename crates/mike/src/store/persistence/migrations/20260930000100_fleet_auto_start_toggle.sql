-- Feature toggle: fleet_auto_start (global, default ON). When it is on, a
-- fleet starts by itself as soon as its definition passes every readiness
-- check. When it is off, the fleet waits at `ready` for the operator's
-- go-ahead (`mike fleet start`, POST /api/fleets/{fleet}/start).
--
-- Default ON by the operator's decision of 2026-09-29. It changes nothing on
-- the wire: it never touches a proxied request or response. It only decides
-- whether mike launches the seats of a fleet that a session has already
-- filled in and marked ready.
--
-- No row is inserted: an absent row means the code default, and a default
-- never rewrites a stored value.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20260929000001_improvement_schema.sql (27 keys). Idempotent: DROP IF
-- EXISTS + re-ADD with every existing key plus 'fleet_auto_start'.

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
                       'session_end_shadow', 'launch_brief', 'judge_lessons', 'fleet_auto_start'));
