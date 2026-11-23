-- `proxy.row_changed` (feed table `session_goals`, session_id the session's
-- uuid) when a session's goal, next steps, role, display name or instructions
-- really change, so get_session_goal keeps its answer per session in memory
-- and reads it again only after that session's notice, instead of on every
-- operator prompt, response and briefing (about 264 reads an hour across the
-- machines before).
--
-- A write that moves only updated_at, set_by or goal_source is silent. The
-- existing display-name trigger (20261120167600) keeps sending the session's
-- `sessions` notice for readers of session identity; this one is separate so
-- those readers are not woken by goal and next-steps rewrites.
--
-- A session delete removes its goal row by cascade after the session row is
-- gone, so the goal row's trigger cannot resolve the uuid; the second trigger
-- sends the notice from the session row itself.
CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_goals()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    session uuid;
BEGIN
    SELECT s.session_uuid INTO session
      FROM sessions.sessions s
     WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    IF session IS NULL THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_goals',
        'id',         session::text,
        'session_id', session::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_goals_of_deleted_session()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_goals',
        'id',         OLD.session_uuid::text,
        'session_id', OLD.session_uuid::text,
        'op',         'DELETE'
    )::text);
    RETURN NULL;
END; $$;

DROP TRIGGER IF EXISTS trg_session_goals_row_changed ON sessions.session_goals;
CREATE TRIGGER trg_session_goals_row_changed
AFTER INSERT OR DELETE ON sessions.session_goals
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_goals();

DROP TRIGGER IF EXISTS trg_session_goals_row_changed_upd ON sessions.session_goals;
CREATE TRIGGER trg_session_goals_row_changed_upd
AFTER UPDATE OF goal, next_steps, role, display_name, instructions ON sessions.session_goals
FOR EACH ROW
WHEN ((OLD.goal, OLD.next_steps, OLD.role, OLD.display_name, OLD.instructions)
      IS DISTINCT FROM
      (NEW.goal, NEW.next_steps, NEW.role, NEW.display_name, NEW.instructions))
EXECUTE FUNCTION mike.notify_row_changed_session_goals();

DROP TRIGGER IF EXISTS trg_sessions_session_goals_row_changed_delete ON sessions.sessions;
CREATE TRIGGER trg_sessions_session_goals_row_changed_delete
AFTER DELETE ON sessions.sessions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_goals_of_deleted_session();
