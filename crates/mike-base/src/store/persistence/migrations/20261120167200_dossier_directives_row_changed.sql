-- `proxy.row_changed` for new directive rows in `memory.dossier_entries`, so
-- the semantic worker on every machine learns at once that a directive needs
-- its semantic item.
--
-- The worker embedded directives by asking on every tick (every 300 s on each
-- machine) for directive entries without an item for the current embedder and
-- builder version. On 2026-11-22 that statement ran about 66 times an hour
-- across the five machines and returned nothing: directives are written a few
-- times a day, and every one already had its item. The worker now asks once at
-- start, again when the embedder changes, and otherwise only after this notice
-- (or a listener resync). Only directive inserts notify, so the commit-time
-- NOTIFY lock is taken a few times a day.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_dossier_directives()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = NEW.session_id;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'dossier_directives',
        'id',         NEW.id::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_dossier_directives_row_changed
AFTER INSERT ON memory.dossier_entries
FOR EACH ROW
WHEN (NEW.kind = 'directive')
EXECUTE FUNCTION mike.notify_row_changed_dossier_directives();
