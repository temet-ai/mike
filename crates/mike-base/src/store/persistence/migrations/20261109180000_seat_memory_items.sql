-- memory.seat_memory_items: the output of memory.seat_memory(project) for every project with
-- seat launches, kept so search reads the seat tier through an index.
--
-- memory.seat_memory(project) costs about 140 ms for the busiest project on the capture data,
-- over the 50 ms read budget before any text match, and matching its 7 MB of text on the fly
-- adds about a second. So the seat_memory_index worker stores what the function returns, and
-- search reads this table. The rules for what enters the seat tier stay in the function only.
--
-- The worker refreshes when the 'seat_memory_sources' generation moves (bumped once per
-- statement that adds a launch, a fleet event or a mail message) and on a slow backstop sweep
-- for the sources that change without one (seat turns closing, notes, thread entries).
--
-- Feature toggle seat_memory_index (global, live disable, default off) gates the worker.
-- config.feature_toggles.key carries a closed CHECK constraint, last widened by
-- 20261109170002_memory_keeper_toggles.sql (48 keys); this adds seat_memory_index.

SET LOCAL lock_timeout = '10s';

CREATE TABLE memory.seat_memory_items (
    project_id       bigint      NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    item_kind        text        NOT NULL,
    source_ref       text        NOT NULL,
    session_uuid     uuid        NOT NULL,
    session_id       bigint,
    run_id           text,
    seat             text,
    task             text,
    author           text        NOT NULL,
    from_session     uuid,
    from_role        text,
    at               timestamptz,
    text             text        NOT NULL,
    repeat_count     integer     NOT NULL,
    report_filed     boolean     NOT NULL,
    verdict_accepted boolean     NOT NULL,
    refreshed_at     timestamptz NOT NULL DEFAULT now(),
    search_tsv       tsvector    GENERATED ALWAYS AS (to_tsvector('english'::regconfig, text)) STORED,
    PRIMARY KEY (project_id, item_kind, source_ref, session_uuid)
);

CREATE INDEX idx_seat_memory_items_search_tsv ON memory.seat_memory_items USING gin (search_tsv);

COMMENT ON TABLE memory.seat_memory_items IS
    'What memory.seat_memory(project) returns, per project with seat launches, refreshed by the seat_memory_index worker. Seat text is data, never operator speech; only knowledge search reads it.';

INSERT INTO memory.change_generations (name) VALUES ('seat_memory_sources')
ON CONFLICT (name) DO NOTHING;

CREATE TRIGGER trg_launches_seat_memory_generation
    AFTER INSERT ON launch.launches
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation('seat_memory_sources');
CREATE TRIGGER trg_fleet_events_seat_memory_generation
    AFTER INSERT ON launch.fleet_events
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation('seat_memory_sources');
CREATE TRIGGER trg_mailbox_messages_seat_memory_generation
    AFTER INSERT ON mailbox.messages
    FOR EACH STATEMENT EXECUTE FUNCTION memory.bump_change_generation('seat_memory_sources');

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
                       'shell_output_compression',
                       'memory_keeper', 'client_serves_memory_keeper',
                       'seat_memory_index'));
