-- Scheduled job run history (crates/mike/src/scheduled_jobs/). Until now each
-- proxy kept its job runs in memory only, so a restart showed every job as
-- "never run", forgot failure backoff and could run a slot that already ran.
-- The worker writes one row when a run finishes and reads the latest row per
-- job when it starts, to restore the status panel and the schedule.
--
-- consecutive_failures and retry_after are the schedule's backoff state right
-- after the run was recorded, so a restart restores them exactly.
-- A manual run ('manual' trigger, started from the home page) does not use up
-- the schedule's period slot; the slot is restored from the latest 'schedule'
-- row's started_at.

CREATE SCHEMA IF NOT EXISTS jobs;

CREATE TABLE jobs.runs (
    id                   bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_name             text        NOT NULL CHECK (btrim(job_name) <> ''),
    machine_uuid         uuid,
    trigger              text        NOT NULL CHECK (trigger IN ('schedule', 'manual')),
    started_at           timestamptz NOT NULL,
    finished_at          timestamptz NOT NULL,
    duration_ms          bigint      NOT NULL CHECK (duration_ms >= 0),
    result_kind          text        NOT NULL CHECK (result_kind IN
                             ('succeeded', 'alarmed', 'exited', 'signalled', 'timed_out', 'unrunnable')),
    exit_code            integer,
    result_detail        text,
    output_tail          text        NOT NULL DEFAULT '',
    consecutive_failures integer     NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    retry_after          timestamptz,
    CHECK (finished_at >= started_at),
    CHECK ((exit_code IS NOT NULL) = (result_kind IN ('succeeded', 'alarmed', 'exited')))
);

CREATE INDEX idx_jobs_runs_job_started ON jobs.runs (job_name, started_at DESC);

COMMENT ON TABLE jobs.runs IS
    'One row per finished scheduled job run (crates/mike/src/scheduled_jobs/jobs.rs '
    'JOBS). Written by the job''s worker when the run ends; the latest row per job '
    'seeds the worker''s status and schedule when the proxy starts.';
COMMENT ON COLUMN jobs.runs.job_name IS
    'Job.name, e.g. learning_promote, cargo_sweep.';
COMMENT ON COLUMN jobs.runs.machine_uuid IS
    'machine_uuid of the proxy that ran the job (machine.json). NULL when that '
    'proxy had no machine identity. Per-machine jobs restore only their own '
    'machine''s rows; leased (one-machine) jobs restore from any machine.';
COMMENT ON COLUMN jobs.runs.trigger IS
    'schedule: started by the job''s cadence. manual: started from the SPA by '
    'POST /api/scheduled-jobs/{name}/run.';
COMMENT ON COLUMN jobs.runs.result_kind IS
    'scheduled_jobs::runner::RunResult variant: succeeded, alarmed (finished and '
    'raised an alarm), exited (non-zero), signalled, timed_out, unrunnable.';
COMMENT ON COLUMN jobs.runs.exit_code IS
    'Script exit code; set exactly for succeeded, alarmed and exited.';
COMMENT ON COLUMN jobs.runs.result_detail IS
    'Why an unrunnable run could not start; NULL otherwise.';
COMMENT ON COLUMN jobs.runs.output_tail IS
    'Last lines of the script''s stdout and stderr, newline-joined.';
COMMENT ON COLUMN jobs.runs.consecutive_failures IS
    'Failed runs in a row, including this one, after it was recorded.';
COMMENT ON COLUMN jobs.runs.retry_after IS
    'Earliest time the schedule tries again after a failure; NULL when no backoff.';
