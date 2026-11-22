-- `proxy.row_changed` for workspace.questions, wright.repairs and
-- wright.followups, so three readers that found nothing can wait for a change instead of asking again.
--
-- workspace.questions: the change record's standing answer read a session's
-- open questions on every call, about 1800 times an hour, and found none.
-- A session's empty answer is now kept until a question to that session is
-- asked or changes. The payload names the recipient (`session_id`), the
-- session whose answer changes; a question to a task alone names none.
--
-- wright.repairs: every machine's repair worker read the landed repairs and
-- marked healed ones every 2 minutes with nothing to follow. It now follows
-- only while a repair is landed or deployed, or after a repair changes.
--
-- wright.followups: the follow-ups and repair workers asked every minute
-- whether a fixer was handed off, about 90 times an hour, and none was. The
-- empty answer is now kept until a follow-up is added or changes state.
-- All three tables are written a few times a day, so the NOTIFY cost is
-- negligible.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_workspace_questions()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    recipient text;
BEGIN
    SELECT s.session_uuid::text INTO recipient
      FROM sessions.sessions s
     WHERE s.id = NEW.to_session_id;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'workspace_questions',
        'id',         NEW.question_uuid::text,
        'session_id', recipient,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_workspace_questions_row_changed
AFTER INSERT ON workspace.questions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_workspace_questions();

CREATE OR REPLACE TRIGGER trg_workspace_questions_row_changed_update
AFTER UPDATE ON workspace.questions
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_workspace_questions();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_repairs()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_repairs',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_repairs_row_changed
AFTER INSERT ON wright.repairs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_repairs();

CREATE OR REPLACE TRIGGER trg_wright_repairs_row_changed_update
AFTER UPDATE OF state ON wright.repairs
FOR EACH ROW WHEN (OLD.state IS DISTINCT FROM NEW.state)
EXECUTE FUNCTION mike.notify_row_changed_wright_repairs();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_wright_followups()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'wright_followups',
        'id',         NEW.id::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_wright_followups_row_changed
AFTER INSERT ON wright.followups
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_wright_followups();

CREATE OR REPLACE TRIGGER trg_wright_followups_row_changed_update
AFTER UPDATE OF state ON wright.followups
FOR EACH ROW WHEN (OLD.state IS DISTINCT FROM NEW.state)
EXECUTE FUNCTION mike.notify_row_changed_wright_followups();
