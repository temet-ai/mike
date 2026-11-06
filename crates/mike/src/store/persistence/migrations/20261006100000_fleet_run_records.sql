-- One measured record per fleet run, written when the run closes, and the
-- carry-over of what a seat learned into the next run's templates. Everything
-- here only adds; nothing existing changes.
--
-- JSONB: `seats`, `mail_by_pair`, `tools_offered` and `tools_used` are display
-- documents read whole by GET /api/fleets/{id}/record; every number a query
-- filters or compares on is a typed column beside them.

CREATE TABLE IF NOT EXISTS launch.run_records (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fleet_uuid uuid NOT NULL UNIQUE REFERENCES launch.fleets(fleet_uuid) ON DELETE CASCADE,
    run_id text NOT NULL UNIQUE REFERENCES launch.runs(run_id) ON DELETE CASCADE,
    project_id bigint REFERENCES projects.projects(id) ON DELETE SET NULL,
    measure_version text NOT NULL,
    started_at timestamptz NOT NULL,
    closed_at timestamptz NOT NULL DEFAULT now(),
    seat_count integer NOT NULL CHECK (seat_count >= 0),
    worker_count integer NOT NULL CHECK (worker_count >= 0),
    working_minutes double precision NOT NULL CHECK (working_minutes >= 0),
    waiting_minutes double precision NOT NULL CHECK (waiting_minutes >= 0),
    first_commit_median_minutes double precision,
    first_commit_longest_minutes double precision,
    reports bigint NOT NULL CHECK (reports >= 0),
    reports_answered bigint NOT NULL CHECK (reports_answered >= 0),
    report_to_verdict_median_minutes double precision,
    report_to_verdict_longest_minutes double precision,
    worker_tool_calls bigint NOT NULL CHECK (worker_tool_calls >= 0),
    worker_git_reads bigint NOT NULL CHECK (worker_git_reads >= 0),
    git_reads_per_100_calls double precision,
    cargo_commands bigint NOT NULL CHECK (cargo_commands >= 0),
    cargo_repeats_across_seats bigint NOT NULL CHECK (cargo_repeats_across_seats >= 0),
    questions_asked bigint NOT NULL CHECK (questions_asked >= 0),
    questions_answered bigint NOT NULL CHECK (questions_answered >= 0),
    forks bigint NOT NULL CHECK (forks >= 0),
    forks_waited bigint NOT NULL CHECK (forks_waited >= 0),
    seats jsonb NOT NULL,
    mail_by_pair jsonb NOT NULL,
    tools_offered jsonb NOT NULL,
    tools_used jsonb NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_run_records_project_id
    ON launch.run_records (project_id) WHERE project_id IS NOT NULL;
COMMENT ON TABLE launch.run_records IS
    'The measures of one closed fleet run, computed from the capture tables by the close (measure_version names the queries); one row per fleet.';

INSERT INTO memory.lesson_sources (code) VALUES ('carry') ON CONFLICT (code) DO NOTHING;

CREATE TABLE IF NOT EXISTS memory.carried_lessons (
    lesson_id bigint PRIMARY KEY REFERENCES memory.lessons(id) ON DELETE CASCADE,
    project_id bigint NOT NULL REFERENCES projects.projects(id) ON DELETE CASCADE,
    discipline text NOT NULL,
    tag text NOT NULL,
    source_note_uuid uuid NOT NULL UNIQUE,
    runs_given integer NOT NULL DEFAULT 0 CHECK (runs_given >= 0),
    later_findings integer NOT NULL DEFAULT 0 CHECK (later_findings >= 0),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_carried_lessons_project_discipline_tag
    ON memory.carried_lessons (project_id, discipline, tag);
COMMENT ON TABLE memory.carried_lessons IS
    'A session note marked data.carry that became a lesson candidate. runs_given and later_findings move only when a template is warmed with the lesson and when a later note carries its tag.';

CREATE TABLE IF NOT EXISTS memory.carried_lesson_runs (
    lesson_id bigint NOT NULL REFERENCES memory.carried_lessons(lesson_id) ON DELETE CASCADE,
    run_id text NOT NULL,
    given_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (lesson_id, run_id)
);

CREATE TABLE IF NOT EXISTS memory.carried_lesson_findings (
    lesson_id bigint NOT NULL REFERENCES memory.carried_lessons(lesson_id) ON DELETE CASCADE,
    note_uuid uuid NOT NULL,
    found_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (lesson_id, note_uuid)
);
