-- A session's display name lives in sessions.session_goals, and readers that
-- keep a session's identity in memory (id, uuid, name, cwd, project) drop it
-- on the session's `sessions` row_changed notice. The sessions trigger covers
-- cwd and project; this one sends the same notice when the display name is
-- set, changed or removed.
CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_goal_name()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    session uuid;
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.display_name IS NOT DISTINCT FROM OLD.display_name THEN
        RETURN NULL;
    END IF;
    SELECT s.session_uuid INTO session
      FROM sessions.sessions s
     WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    IF session IS NULL THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'sessions',
        'id',         session::text,
        'session_id', session::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

DROP TRIGGER IF EXISTS trg_session_goal_name_row_changed ON sessions.session_goals;
CREATE TRIGGER trg_session_goal_name_row_changed
AFTER INSERT OR DELETE OR UPDATE OF display_name ON sessions.session_goals
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_goal_name();
