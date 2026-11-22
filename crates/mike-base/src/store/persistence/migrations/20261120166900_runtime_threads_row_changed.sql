-- `proxy.row_changed` for new `runtime.threads` rows, so the mail outbox
-- worker on every machine learns at once that a Codex session was bound.
--
-- The outbox only carries mail to or from a Codex session. While no Codex
-- thread is bound the worker has nothing to mirror or deliver, so it waits
-- for this notice instead of sweeping on every mailbox, session or launch
-- change (about 5,500 empty sweeps an hour on 2026-11-22, with no Codex
-- binding in the database). Bindings are written once per Codex thread, so
-- the commit-time NOTIFY lock is negligible.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_runtime_threads()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = NEW.session_id;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'runtime_threads',
        'id',         NEW.provider_thread_id,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_runtime_threads_row_changed
AFTER INSERT ON runtime.threads
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_runtime_threads();
