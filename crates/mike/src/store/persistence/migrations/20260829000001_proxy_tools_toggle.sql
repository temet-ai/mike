-- Feature toggle: proxy_tools (session-scoped, default OFF). Gates
-- `crate::proxy_tools` — GitHub issue #204: the proxy declares its own
-- tool definitions into the outbound request's `tools[]` and answers the
-- model's calls against them itself, so a session gets mike's tooling
-- without any client-side MCP configuration.
--
-- This is the one toggle seeded DISABLED, against the no-default-off rule
-- every other row here follows. Answering a call means recognising it
-- before the client sees the response, which means buffering the upstream
-- SSE stream, which means the reply arrives all at once instead of
-- streaming. `weasel_intercept.forbidden_words` is empty in this
-- deployment, so nothing buffers responses today, and seeding this
-- enabled would take streaming away from every turn in every session to
-- pay for a tool that fires on almost none of them. Releasing the buffer
-- as soon as the first content block proves not to be a proxy-tool call
-- removes that cost; that is the work that should let this default on.
-- Rationale in full: artifacts/2026-08-29/proxy-declared-tools-design.md.
--
-- config.feature_toggles.key carries a closed CHECK constraint (introduced
-- in 20260615000003_feature_toggles.sql, last extended in
-- 20260819010000_retention_sweep_toggle.sql). Re-add whitelists
-- 'proxy_tools' alongside the existing keys. Idempotent: DROP IF EXISTS +
-- ON CONFLICT DO NOTHING.

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
        'retention_sweep',
        'proxy_tools'
    ));

INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('proxy_tools', false)
ON CONFLICT (key) DO NOTHING;
