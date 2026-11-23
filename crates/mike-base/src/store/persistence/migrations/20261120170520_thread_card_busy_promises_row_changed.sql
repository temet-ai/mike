-- `proxy.row_changed` (feed table `thread_card_busy_sources`) when a promise in
-- `memory.dossier_entries` is recorded or closed for a session linked to a
-- work thread. Migrations 20261120170521 (live turn verdicts) and
-- 20261120170522 (thread-tagged session notes) send the same notice; one table
-- per migration, so no migration locks two busy tables at once.
--
-- The memory keeper swept for cards behind their busy sources every 60 s on
-- the machine holding the memory_keeper_cards lease. On 2026-11-23 that sweep
-- (queryid -60782104982633804) ran about 60 times an hour and returned
-- nothing. Its rows come only from a thread-linked session's promise, live
-- verdict or thread-tagged note moving past the card's source_max_at, so the
-- keeper now sweeps after this notice, a listener resync or a 600 s backstop.
-- The payload is the same for every row, so a dossier pass that records many
-- promises in one transaction sends one notice.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_thread_card_busy_session()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM sessions.sessions se
          JOIN work.thread_sessions ts ON ts.session_uuid = se.session_uuid
         WHERE se.id = NEW.session_id
    ) THEN
        PERFORM pg_notify('proxy.row_changed', json_build_object(
            'table',      'thread_card_busy_sources',
            'id',         TG_TABLE_SCHEMA || '.' || TG_TABLE_NAME,
            'session_id', NULL,
            'op',         TG_OP
        )::text);
    END IF;
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_dossier_entries_thread_card_busy_inserted
AFTER INSERT ON memory.dossier_entries
FOR EACH ROW
WHEN (NEW.kind = 'promise')
EXECUTE FUNCTION mike.notify_row_changed_thread_card_busy_session();

CREATE OR REPLACE TRIGGER trg_dossier_entries_thread_card_busy_updated
AFTER UPDATE OF recorded_at, close_recorded_at ON memory.dossier_entries
FOR EACH ROW
WHEN (NEW.kind = 'promise'
      AND (OLD.recorded_at IS DISTINCT FROM NEW.recorded_at
           OR OLD.close_recorded_at IS DISTINCT FROM NEW.close_recorded_at))
EXECUTE FUNCTION mike.notify_row_changed_thread_card_busy_session();
