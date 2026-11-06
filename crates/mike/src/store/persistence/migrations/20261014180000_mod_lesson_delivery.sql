-- Feature toggle: mod_lesson_delivery (global, default OFF). When on, the mike
-- mod adds lessons as hook context at prompt submit, which Claude Code keeps in
-- its own transcript, and the proxy stops injecting lessons into the sessions
-- the mod serves. A stable share of sessions stays on proxy injection as the
-- control group. memory.lesson_delivery_arms records which arm each session is
-- in and what the mod delivered to it.
--
-- No toggle row is inserted: an absent row means the code default.
--
-- config.feature_toggles.key carries a closed CHECK constraint, last widened
-- by 20261006010000_native_peer_mail_toggle.sql (32 keys).

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
                       'native_peer_mail', 'mod_lesson_delivery'));

CREATE TABLE IF NOT EXISTS memory.lesson_delivery_arms (
    session_uuid      uuid        PRIMARY KEY,
    arm               text        NOT NULL CHECK (arm IN ('mod', 'control')),
    decided_at        timestamptz NOT NULL DEFAULT now(),
    last_delivery_at  timestamptz,
    deliveries        integer     NOT NULL DEFAULT 0,
    lessons_delivered integer     NOT NULL DEFAULT 0,
    bytes_delivered   bigint      NOT NULL DEFAULT 0
);

COMMENT ON TABLE memory.lesson_delivery_arms IS 'Which way a session receives lessons while mod_lesson_delivery is on: mod (the mike mod adds them as hook context, kept in the client transcript) or control (the proxy keeps injecting them). One row per session that asked, written by POST /api/lessons/for-prompt.';

CREATE INDEX IF NOT EXISTS idx_lesson_delivery_arms_decided_at ON memory.lesson_delivery_arms (decided_at DESC);
