-- LISTEN/NOTIFY triggers for the mailbox surface, following the pattern in
-- 20260614000002_interventions_row_change_trigger.sql. The read-API schema
-- is `mike` (renamed from `claude_proxy` by 20260718000000), so both
-- functions live there.
--
-- mailbox.messages: session_id resolves to the RECIPIENT (to_session_id) —
-- that is the session whose inbox a live SPA view needs to reload.
-- mailbox.threads: session-independent like filter_rules/prompt_compositions
-- (a thread's participants are derived from its messages, not stored on the
-- thread row itself), so no session_id is emitted.

CREATE FUNCTION mike.notify_row_changed_mailbox_messages()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = COALESCE(NEW.to_session_id, OLD.to_session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'mailbox_messages',
        'id',         COALESCE(NEW.message_uuid, OLD.message_uuid)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_mailbox_messages_row_changed
AFTER INSERT OR UPDATE OR DELETE ON mailbox.messages
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_mailbox_messages();


CREATE FUNCTION mike.notify_row_changed_mailbox_threads()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table', 'mailbox_threads',
        'id',    COALESCE(NEW.thread_uuid, OLD.thread_uuid)::text,
        'op',    TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_mailbox_threads_row_changed
AFTER INSERT OR UPDATE OR DELETE ON mailbox.threads
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_mailbox_threads();
