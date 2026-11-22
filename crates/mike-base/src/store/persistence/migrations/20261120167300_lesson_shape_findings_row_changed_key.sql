-- 20261120166400 pointed the memory.lesson_shape_findings triggers at
-- mike.notify_row_changed_lessons(), which reads NEW.id / OLD.id. That table's key is
-- lesson_id and it has no id column, so every insert, delete or watched update of a shape
-- finding raised `record "new" has no field "id"` and rolled its transaction back. Creating a
-- lesson inserts its shape finding in the same transaction, so lesson creation failed.
--
-- Shape findings get their own function that reports lesson_id. The payload still names the
-- 'lessons' table: the lesson worker treats a lesson and a shape finding change alike.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_lesson_shape_findings()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'lessons',
        'id',         COALESCE(NEW.lesson_id, OLD.lesson_id)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_lesson_shape_findings_row_changed
AFTER INSERT OR DELETE ON memory.lesson_shape_findings
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_lesson_shape_findings();

CREATE OR REPLACE TRIGGER trg_lesson_shape_findings_row_changed_update
AFTER UPDATE ON memory.lesson_shape_findings
FOR EACH ROW WHEN (OLD.resolution IS DISTINCT FROM NEW.resolution
                   OR OLD.lesson_id IS DISTINCT FROM NEW.lesson_id)
EXECUTE FUNCTION mike.notify_row_changed_lesson_shape_findings();
