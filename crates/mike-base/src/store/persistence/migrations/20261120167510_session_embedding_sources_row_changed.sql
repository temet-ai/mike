-- `proxy.row_changed` (feed table `session_embedding_sources`) when a
-- session's embedding source moves forward: its chunk mark in
-- `memory.session_chunk_marks` is inserted or advanced (a turn's chunks were
-- embedded), or its `sessions.sessions.overview_generated_at` changes. The
-- session vector sweep waits for it before it asks again for sessions whose
-- embedding is missing or older than its sources.
--
-- The sweep asked on every pass (every 300 s on each machine). On 2026-11-22
-- that statement (queryid -7164945423352091624) ran about 70 times an hour
-- across the five machines and returned nothing. Its rows come only from these
-- two columns moving past `memory.session_embeddings.source_updated_at`, so it
-- now asks once at start and otherwise only after this notice, a listener
-- resync or a 3600 s backstop. A mark moves once per embedded turn and an
-- overview a few times an hour, so the commit-time NOTIFY lock is negligible.
-- `sessions.sessions` keeps its wider `trg_sessions_row_changed` for the SPA;
-- that one also fires on status changes and would wake the sweep on every
-- request.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_chunk_marks()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
      FROM sessions.sessions s
     WHERE s.id = NEW.session_id;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_embedding_sources',
        'id',         NEW.session_id::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_session_chunk_marks_row_changed
AFTER INSERT OR UPDATE OF chunks_embedded_at ON memory.session_chunk_marks
FOR EACH ROW
EXECUTE FUNCTION mike.notify_row_changed_session_chunk_marks();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_session_overviews()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'session_embedding_sources',
        'id',         NEW.id::text,
        'session_id', NEW.session_uuid::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_session_overviews_row_changed
AFTER UPDATE OF overview_generated_at ON sessions.sessions
FOR EACH ROW
WHEN (OLD.overview_generated_at IS DISTINCT FROM NEW.overview_generated_at)
EXECUTE FUNCTION mike.notify_row_changed_session_overviews();
