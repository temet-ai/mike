-- A change to a mailbox message now also sends a `proxy.row_changed` notice
-- that names the sender, as well as the one that names the recipient. A
-- session's inbox and outbox lists are kept in memory per session until that
-- session's mailbox generation moves, so a delivery, consumption or dead
-- letter of mail the session sent has to move the sender's generation too.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_mailbox_messages()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    recipient_id bigint := COALESCE(NEW.to_session_id, OLD.to_session_id);
    sender_id bigint := COALESCE(NEW.from_session_id, OLD.from_session_id);
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
    FROM sessions.sessions s
    WHERE s.id = recipient_id;
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'mailbox_messages',
        'id',         COALESCE(NEW.message_uuid, OLD.message_uuid)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    IF sender_id IS NOT NULL AND sender_id IS DISTINCT FROM recipient_id THEN
        SELECT s.session_uuid::text INTO sess_uuid
        FROM sessions.sessions s
        WHERE s.id = sender_id;
        IF sess_uuid IS NOT NULL THEN
            PERFORM pg_notify('proxy.row_changed', json_build_object(
                'table',      'mailbox_messages',
                'id',         COALESCE(NEW.message_uuid, OLD.message_uuid)::text,
                'session_id', sess_uuid,
                'op',         TG_OP
            )::text);
        END IF;
    END IF;
    RETURN NULL;
END; $$;
