-- Wright Watch (temet-ai/mike#72): mike's own checks over its logs, capture database, MCP tool
-- errors, page errors, hook errors, workers and session reports, and the problems they open.
--
-- wright.check_runs: one row per run of one compiled check, with its outcome. 'unknown' means
-- the check could not read its evidence (a failed query, a missing log); it never closes a
-- problem.
-- wright.problems: one row per (check, scope, resource), whatever the number of scans or
-- workers that found it. 'healthy' only after a complete run of the check no longer finds it.
-- wright.episodes: each time a problem is open. A recurrence after a healthy run is a new
-- episode of the same problem; at most one open episode per problem.
-- wright.client_errors: page load failures and client errors the web page reports.
--
-- Feature toggle wright (global, live disable, default off) gates the worker only; the problems
-- API reads these tables whether it is on or off. config.feature_toggles.key carries a closed
-- CHECK constraint, last widened by 20261109180000_seat_memory_items.sql; this adds wright.

SET LOCAL lock_timeout = '10s';

CREATE SCHEMA IF NOT EXISTS wright;

CREATE TABLE wright.check_runs (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    check_name   text        NOT NULL,
    source       text        NOT NULL,
    cadence      text        NOT NULL CHECK (cadence IN ('event', 'regular', 'daily')),
    started_at   timestamptz NOT NULL,
    finished_at  timestamptz NOT NULL DEFAULT now(),
    outcome      text        NOT NULL CHECK (outcome IN ('pass', 'fail', 'unknown')),
    findings     integer     NOT NULL DEFAULT 0 CHECK (findings >= 0),
    error        text,
    machine_uuid uuid,
    CONSTRAINT check_runs_unknown_has_error CHECK ((outcome = 'unknown') = (error IS NOT NULL))
);

CREATE INDEX idx_wright_check_runs_check_finished
    ON wright.check_runs (check_name, finished_at DESC);

CREATE TABLE wright.problems (
    id           bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    check_name   text        NOT NULL,
    source       text        NOT NULL,
    scope        text        NOT NULL,
    resource     text        NOT NULL,
    state        text        NOT NULL CHECK (state IN ('open', 'healthy')),
    opened_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    healthy_at   timestamptz,
    CONSTRAINT problems_one_per_check_scope_resource UNIQUE (check_name, scope, resource),
    CONSTRAINT problems_healthy_has_time CHECK ((state = 'healthy') = (healthy_at IS NOT NULL))
);

CREATE INDEX idx_wright_problems_state_seen ON wright.problems (state, last_seen_at DESC, id DESC);
CREATE INDEX idx_wright_problems_seen ON wright.problems (last_seen_at DESC, id DESC);

CREATE TABLE wright.episodes (
    id              bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    problem_id      bigint      NOT NULL REFERENCES wright.problems(id) ON DELETE CASCADE,
    opened_at       timestamptz NOT NULL DEFAULT now(),
    last_seen_at    timestamptz NOT NULL DEFAULT now(),
    closed_at       timestamptz,
    opened_by_run   bigint      NOT NULL REFERENCES wright.check_runs(id) ON DELETE CASCADE,
    last_seen_run   bigint      NOT NULL REFERENCES wright.check_runs(id) ON DELETE CASCADE,
    closed_by_run   bigint      REFERENCES wright.check_runs(id) ON DELETE SET NULL,
    observations    integer     NOT NULL DEFAULT 1 CHECK (observations >= 1),
    evidence        jsonb       NOT NULL
);

CREATE UNIQUE INDEX uq_wright_episodes_one_open ON wright.episodes (problem_id) WHERE closed_at IS NULL;
CREATE INDEX idx_wright_episodes_problem_opened ON wright.episodes (problem_id, opened_at DESC, id DESC);
CREATE INDEX idx_wright_episodes_opened_by_run ON wright.episodes (opened_by_run);
CREATE INDEX idx_wright_episodes_last_seen_run ON wright.episodes (last_seen_run);
CREATE INDEX idx_wright_episodes_closed_by_run ON wright.episodes (closed_by_run) WHERE closed_by_run IS NOT NULL;

CREATE TABLE wright.client_errors (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    received_at timestamptz NOT NULL DEFAULT now(),
    route       text        NOT NULL CHECK (btrim(route) <> ''),
    kind        text        NOT NULL CHECK (kind IN ('load', 'console', 'api', 'unhandled')),
    message     text        NOT NULL CHECK (btrim(message) <> ''),
    detail      jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_wright_client_errors_received ON wright.client_errors (received_at DESC);

COMMENT ON SCHEMA wright IS
    'Wright, mike''s resident engineer: compiled checks over mike''s own signals and the problems they open (temet-ai/mike#71).';

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
                       'seat_memory_index',
                       'wright'));

INSERT INTO config.feature_toggles (key, enabled, operator_set)
VALUES ('wright', false, false)
ON CONFLICT (key) DO NOTHING;
