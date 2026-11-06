-- LISTEN/NOTIFY trigger coverage for the seven durable sources the Session
-- Flow projection (P05) reads: memory.lesson_injections,
-- steering.evaluations, memory.consolidation_actions,
-- memory.consolidation_runs, session_judges.verdicts, llm.calls, and
-- memory.session_embeddings. Follows the established `mike` schema pattern
-- (20260810100002_mailbox_row_change_triggers.sql): one PL/pgSQL function
-- per table, `pg_notify` on the shared `proxy.row_changed` channel, payload
-- `{table, id, session_id?, op}`. `session_id` is the session's UUID
-- (`sessions.sessions.session_uuid`), resolved per table below; a table
-- with no session ownership at all omits the key (JSON absence, not
-- `null`), matching `memory.consolidation_runs` /
-- `filter_rules` / `prompt_compositions` precedent — the listener's
-- `#[serde(default)]` on `session_id` treats both the same.
--
-- Routing per table (see artifacts/2026-09-10/session-flow/mike-live-tasks.md
-- P06's producer-seam table for the full rationale):
--
--   memory.lesson_injections    -- session_id when set; else resolved via
--                                   request_id (ON DELETE SET NULL leaves the
--                                   row behind when its session is deleted,
--                                   with request_id still intact).
--   steering.evaluations        -- session_id always (NOT NULL); the
--                                   subquery still degrades to NULL when a
--                                   cascade delete has already removed the
--                                   parent session row in the same statement.
--   memory.consolidation_actions -- session_uuid is already denormalized on
--                                    the row (GC-survival column) — no join.
--   memory.consolidation_runs   -- no session column at all: always global.
--   session_judges.verdicts     -- session_id always (NOT NULL), subquery
--                                   join like evaluations.
--   llm.calls                   -- session_id is nullable and carries no FK
--                                   (deliberately, per its own migration
--                                   header); the subquery naturally yields
--                                   NULL for an unresolvable or absent id.
--   memory.session_embeddings   -- session_id IS the session UUID (the
--                                   table's own primary key) — no join.

CREATE FUNCTION mike.notify_row_changed_lesson_injections()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);

    IF sess_uuid IS NULL THEN
        SELECT s.session_uuid::text INTO sess_uuid
        FROM requests.requests r
        JOIN sessions.sessions s ON s.id = r.session_id
        WHERE r.id = COALESCE(NEW.request_id, OLD.request_id);
    END IF;

    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'lesson_injections',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_lesson_injections_row_changed
AFTER INSERT OR UPDATE OR DELETE ON memory.lesson_injections
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_lesson_injections();


CREATE FUNCTION mike.notify_row_changed_steering_evaluations()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);

    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'steering_evaluations',
        'id',         COALESCE(NEW.uuid, OLD.uuid)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_steering_evaluations_row_changed
AFTER INSERT OR UPDATE OR DELETE ON steering.evaluations
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_steering_evaluations();


CREATE FUNCTION mike.notify_row_changed_consolidation_actions()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'consolidation_actions',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', COALESCE(NEW.session_uuid, OLD.session_uuid)::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_consolidation_actions_row_changed
AFTER INSERT OR UPDATE OR DELETE ON memory.consolidation_actions
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_consolidation_actions();


CREATE FUNCTION mike.notify_row_changed_consolidation_runs()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'consolidation_runs',
        'id',    COALESCE(NEW.id, OLD.id)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_consolidation_runs_row_changed
AFTER INSERT OR UPDATE OR DELETE ON memory.consolidation_runs
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_consolidation_runs();


CREATE FUNCTION mike.notify_row_changed_session_judges_verdicts()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);

    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_judges_verdicts',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_session_judges_verdicts_row_changed
AFTER INSERT OR UPDATE OR DELETE ON session_judges.verdicts
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_judges_verdicts();


CREATE FUNCTION mike.notify_row_changed_llm_calls()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);

    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'llm_calls',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_llm_calls_row_changed
AFTER INSERT OR UPDATE OR DELETE ON llm.calls
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_llm_calls();


CREATE FUNCTION mike.notify_row_changed_session_embeddings()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_embeddings',
        'id',         COALESCE(NEW.session_id, OLD.session_id)::text,
        'session_id', COALESCE(NEW.session_id, OLD.session_id)::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_session_embeddings_row_changed
AFTER INSERT OR UPDATE OR DELETE ON memory.session_embeddings
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_session_embeddings();
