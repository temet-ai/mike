-- GitHub issue #658 — a thread lives in mike's database, not in a worklog.md file.
-- Stage 1 of the plan: extend the `work` schema that 20260903000002_work_threads.sql
-- created (empty, never wired) so that a thread can be written directly:
-- created, checkpointed, given history entries (briefs and handoffs included),
-- given dependencies and acceptance criteria, and linked to the sessions that
-- worked on it.
--
-- Nothing is dropped and no data is rewritten; every table is empty today. Each
-- statement is idempotent so a re-run after a partial apply is safe.
--
--   work.threads                  goal; source_path becomes optional, because a
--                                  thread created in mike has no file. worktree
--                                  now holds a path relative to the project root.
--   work.thread_state             decisions and the files the next session must
--                                  read (project-relative paths).
--   work.thread_log               what kind of entry it is, its full body and an
--                                  optional JSON payload; entry_date defaults to
--                                  today. ord stays the per-thread sequence, 1..n;
--                                  a direct write takes max(ord) + 1 while holding
--                                  the thread row lock.
--   work.thread_acceptance_criteria  source_path optional, same reason.
--   work.thread_dependencies      new: thread A depends on thread B, both in
--                                  mike, so a dependency survives a rename of the
--                                  file that used to name it.

ALTER TABLE work.threads
    ADD COLUMN IF NOT EXISTS goal text NULL;

ALTER TABLE work.threads
    ALTER COLUMN source_path DROP NOT NULL;

ALTER TABLE work.threads
    DROP CONSTRAINT IF EXISTS threads_status_check;
ALTER TABLE work.threads
    ADD CONSTRAINT threads_status_check
    CHECK (status IN ('active', 'implementing', 'review', 'parked', 'done', 'complete', 'in-progress'));

COMMENT ON COLUMN work.threads.goal IS
    'What the thread is for, in a sentence or two. Set when the thread is created or changed later; not part of the replaced-whole state.';
COMMENT ON COLUMN work.threads.source_path IS
    'The worklog file an import read this thread from. NULL for a thread created directly in mike.';
COMMENT ON COLUMN work.threads.worktree IS
    'Path of the thread''s git worktree, relative to the project root so every machine resolves it against its own location of the project. NULL when the thread has none.';

ALTER TABLE work.thread_state
    ADD COLUMN IF NOT EXISTS decisions text NOT NULL DEFAULT '';
ALTER TABLE work.thread_state
    ADD COLUMN IF NOT EXISTS files text[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN work.thread_state.decisions IS
    'Decisions in force for the thread, replaced whole on every checkpoint like the other state fields.';
COMMENT ON COLUMN work.thread_state.files IS
    'Files the next session must read, as paths relative to the project root.';

ALTER TABLE work.thread_log
    ADD COLUMN IF NOT EXISTS kind text NOT NULL DEFAULT 'checkpoint';
ALTER TABLE work.thread_log
    ADD COLUMN IF NOT EXISTS body text NOT NULL DEFAULT '';
ALTER TABLE work.thread_log
    ADD COLUMN IF NOT EXISTS data jsonb NULL;
ALTER TABLE work.thread_log
    ALTER COLUMN entry_date SET DEFAULT current_date;

ALTER TABLE work.thread_log
    DROP CONSTRAINT IF EXISTS thread_log_kind_check;
ALTER TABLE work.thread_log
    ADD CONSTRAINT thread_log_kind_check
    CHECK (kind IN ('checkpoint', 'decision', 'finding', 'brief', 'handoff', 'note', 'state-replaced'));

COMMENT ON COLUMN work.thread_log.kind IS
    'checkpoint, decision, finding, brief, handoff or note are written by sessions. state-replaced is written by a checkpoint and holds the state it replaced in `data`.';
COMMENT ON COLUMN work.thread_log.body IS
    'The entry in full. `summary` is its first line.';
COMMENT ON COLUMN work.thread_log.data IS
    'Structured payload. A state-replaced entry carries the old state fields; an imported worklog carries {"imported_from": path}.';

ALTER TABLE work.thread_acceptance_criteria
    ALTER COLUMN source_path DROP NOT NULL;

CREATE TABLE IF NOT EXISTS work.thread_dependencies (
    thread_id            bigint      NOT NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    depends_on_thread_id bigint      NOT NULL REFERENCES work.threads(id) ON DELETE CASCADE,
    created_at           timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (thread_id, depends_on_thread_id),
    CONSTRAINT thread_dependencies_no_self_reference
        CHECK (thread_id <> depends_on_thread_id)
);

COMMENT ON TABLE work.thread_dependencies IS
    'Thread thread_id cannot finish before thread depends_on_thread_id (issue #658). Both threads belong to the same project; the persistence layer resolves the dependency by slug.';

CREATE INDEX IF NOT EXISTS idx_work_thread_dependencies_depends_on
    ON work.thread_dependencies (depends_on_thread_id);
