-- `proxy.row_changed` for the three tables whose readers polled them on a timer and almost
-- always found nothing new:
--
-- * `lessons` (memory.lessons and memory.lesson_shape_findings): the lesson worker asked every
--   30 s on every machine which projects hold raw lessons ready to embed (about 350 calls an
--   hour, nearly all empty). It now asks again only after a lesson or a shape finding changed,
--   or when a raw lesson it already saw becomes old enough.
-- * `rejudge_runs` (eval.rejudge_runs): both judge workers asked on every tick for an active
--   rejudge run; one run has ever existed. They now ask again only after a run row changed.
-- * `guard_thresholds` (steering.guard_thresholds): the cross-session worker re-read the
--   thresholds in force every minute; they change only when a threshold pass writes rows.
--
-- Each table is written a few hundred times a day at most, so the commit-time NOTIFY lock is
-- negligible. UPDATE notifies only when a column the readers filter on changed (lessons get
-- thousands of usage-count updates a day that change nothing for them). `session_id` is NULL:
-- none of these is session-scoped, so consumers treat each as a table-wide change.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_lessons()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'lessons',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_lessons_row_changed
AFTER INSERT OR DELETE ON memory.lessons
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_lessons();

CREATE OR REPLACE TRIGGER trg_lessons_row_changed_update
AFTER UPDATE ON memory.lessons
FOR EACH ROW WHEN (OLD.stage_id IS DISTINCT FROM NEW.stage_id
                   OR OLD.embedded_at IS DISTINCT FROM NEW.embedded_at
                   OR OLD.project_id IS DISTINCT FROM NEW.project_id
                   OR OLD.source_id IS DISTINCT FROM NEW.source_id)
EXECUTE FUNCTION mike.notify_row_changed_lessons();

CREATE OR REPLACE TRIGGER trg_lesson_shape_findings_row_changed
AFTER INSERT OR DELETE ON memory.lesson_shape_findings
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_lessons();

CREATE OR REPLACE TRIGGER trg_lesson_shape_findings_row_changed_update
AFTER UPDATE ON memory.lesson_shape_findings
FOR EACH ROW WHEN (OLD.resolution IS DISTINCT FROM NEW.resolution
                   OR OLD.lesson_id IS DISTINCT FROM NEW.lesson_id)
EXECUTE FUNCTION mike.notify_row_changed_lessons();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_rejudge_runs()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'rejudge_runs',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_rejudge_runs_row_changed
AFTER INSERT OR DELETE ON eval.rejudge_runs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_rejudge_runs();

CREATE OR REPLACE TRIGGER trg_rejudge_runs_row_changed_update
AFTER UPDATE ON eval.rejudge_runs
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_rejudge_runs();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_guard_thresholds()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'guard_thresholds',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_guard_thresholds_row_changed
AFTER INSERT OR DELETE ON steering.guard_thresholds
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_guard_thresholds();

CREATE OR REPLACE TRIGGER trg_guard_thresholds_row_changed_update
AFTER UPDATE ON steering.guard_thresholds
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_guard_thresholds();
