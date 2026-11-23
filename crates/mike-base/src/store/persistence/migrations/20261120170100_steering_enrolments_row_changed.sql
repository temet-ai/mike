-- `proxy.row_changed` (feed table `steering_enrolments`) when a session's steerer
-- enrolments change, so a reader can keep a session's active enrolments in memory
-- until they move.
--
-- The steerer fleet asked `list_active_enrolments_for_session` on every captured
-- response, every genuine user prompt and every quiescent turn. On f4417274a it ran
-- about 164 times an hour across the five machines and returned no rows. A session's
-- enrolments are written when it starts (auto-enrol) and by an operator's enrol or
-- disenrol, about 23 rows an hour in all, so the commit-time NOTIFY lock is
-- negligible. `session_id` is the session's uuid; a delete cascaded from a deleted
-- session finds no session row, so the session's own delete sends the notice instead.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_steering_enrolments()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
      FROM sessions.sessions s
     WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    IF sess_uuid IS NULL THEN
        RETURN NULL;
    END IF;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'steering_enrolments',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_steering_enrolments_row_changed
AFTER INSERT OR DELETE ON steering.enrolments
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_steering_enrolments();

CREATE OR REPLACE TRIGGER trg_steering_enrolments_row_changed_update
AFTER UPDATE ON steering.enrolments
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_steering_enrolments();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_enrolments_gone()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'steering_enrolments',
        'id',         OLD.id::text,
        'session_id', OLD.session_uuid::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_sessions_enrolments_row_changed_delete
AFTER DELETE ON sessions.sessions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_enrolments_gone();
