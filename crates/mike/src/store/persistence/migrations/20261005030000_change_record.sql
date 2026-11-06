-- The change record: one append-only table of what happened in a checkout
-- (file changes with who made them and the content before, commits, pushes,
-- checks, verdicts, notes, mail, briefs), each event with the paths it
-- concerns, and one row per session and checkout holding the last event that
-- session was shown. Everything here only adds; nothing existing changes.
--
-- Both toggles default OFF and no row is inserted for them: an absent row
-- means the code default, and a default never rewrites a stored value.
--
-- JSONB: `detail` is the per-kind payload (the tool_use id of an edit, the
-- candidate sessions of a shell change, the note or mail id); every field a
-- query filters or sorts on is a typed column beside it.
--
-- before_sha and after_sha are git blob ids kept as a fingerprint of the
-- content on each side of a change, never as a key; neither is indexed.

CREATE TABLE IF NOT EXISTS workspace.change_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    checkout text NOT NULL,
    at timestamptz NOT NULL,
    kind text NOT NULL CHECK (kind IN (
        'edit', 'shell_change', 'commit', 'push', 'check',
        'verdict', 'note', 'mail', 'brief', 'task_state')),
    session_id bigint REFERENCES sessions.sessions(id) ON DELETE SET NULL,
    run_id text,
    task text,
    paths text[] NOT NULL,
    subject text NOT NULL,
    detail jsonb NOT NULL DEFAULT '{}'::jsonb,
    before_sha text,
    after_sha text
);

CREATE INDEX IF NOT EXISTS idx_change_events_checkout_id
    ON workspace.change_events (checkout, id);
CREATE INDEX IF NOT EXISTS idx_change_events_project_id
    ON workspace.change_events (project_id);
CREATE INDEX IF NOT EXISTS idx_change_events_session_id
    ON workspace.change_events (session_id) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_change_events_paths
    ON workspace.change_events USING gin (paths);

COMMENT ON TABLE workspace.change_events IS
    'Append-only record of what happened in a checkout. checkout is the repository root on the machine that recorded it; paths are relative to it.';

CREATE TABLE IF NOT EXISTS workspace.change_places (
    session_id bigint NOT NULL REFERENCES sessions.sessions(id) ON DELETE CASCADE,
    checkout text NOT NULL,
    last_event_id bigint NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, checkout)
);

COMMENT ON TABLE workspace.change_places IS
    'The last change_events id each session was shown for a checkout; what changed since it last looked is the range after it.';

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
                       'repeated_notices', 'change_record', 'change_record_tail'));
