CREATE TABLE IF NOT EXISTS launch.fleet_events (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES launch.runs(run_id) ON DELETE CASCADE,
    task         TEXT,
    session_uuid UUID,
    kind         TEXT NOT NULL CHECK (kind IN (
        'plan_versioned',
        'seat_launch_requested',
        'seat_launch_confirmed',
        'seat_launch_failed',
        'seat_first_request',
        'seat_heartbeat_stale',
        'seat_process_gone',
        'seat_closed',
        'seat_woken',
        'commit_landed',
        'tree_broken_by',
        'check_queued',
        'check_finished',
        'floor_passed',
        'floor_failed',
        'report_filed',
        'verdict_recorded',
        'criterion_suspect',
        'question_asked',
        'question_answered',
        'problem_opened',
        'problem_decided',
        'problem_escalated',
        'stop_requested',
        'operator_touched',
        'resource_admitted'
    )),
    payload      JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE launch.fleet_events IS 'The append-only record of a fleet run: one row per thing that happened, never updated. Every state a fleet shows is a fold over these rows in id order. task and session_uuid name what the event is about when it is about one; both stay NULL for an event about the whole run. The payload carries ids and shas, never captured text.';
COMMENT ON COLUMN launch.fleet_events.payload IS 'JSONB because each kind has its own small shape (a commit sha, a receipt id, a note id) that is decoded whole in Rust by kind; nothing filters or joins on a field inside it.';

CREATE INDEX IF NOT EXISTS idx_fleet_events_run_id ON launch.fleet_events (run_id, id);
CREATE INDEX IF NOT EXISTS idx_fleet_events_run_task_kind ON launch.fleet_events (run_id, task, kind) WHERE task IS NOT NULL;
