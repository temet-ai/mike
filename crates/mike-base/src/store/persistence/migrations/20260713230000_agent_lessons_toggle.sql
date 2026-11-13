-- Feature toggle: agent_lessons (session-scoped, default ON — no-default-off
-- rule). Gates injection of agent-scoped lessons (memory.lessons.agent_id,
-- resolved via agents.aliases) into that agent's own subagent requests.

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
        'agent_lessons'
    ));

INSERT INTO config.feature_toggles (key, enabled) VALUES
    ('agent_lessons', true)
ON CONFLICT (key) DO NOTHING;
