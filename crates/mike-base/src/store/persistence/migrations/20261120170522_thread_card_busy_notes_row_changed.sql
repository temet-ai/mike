-- `proxy.row_changed` (feed table `thread_card_busy_sources`) when a decision
-- or finding in `workspace.session_notes` carrying a `thread:<slug>` tag is
-- written or changed. See 20261120170520 for why the memory keeper waits on
-- this notice.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_thread_card_busy_note()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM unnest(NEW.tags) AS tag WHERE tag LIKE 'thread:%') THEN
        PERFORM pg_notify('proxy.row_changed', json_build_object(
            'table',      'thread_card_busy_sources',
            'id',         'workspace.session_notes',
            'session_id', NULL,
            'op',         TG_OP
        )::text);
    END IF;
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_session_notes_thread_card_busy_written
AFTER INSERT OR UPDATE OF kind, tags, updated_at ON workspace.session_notes
FOR EACH ROW
WHEN (NEW.kind IN ('decision', 'finding'))
EXECUTE FUNCTION mike.notify_row_changed_thread_card_busy_note();
