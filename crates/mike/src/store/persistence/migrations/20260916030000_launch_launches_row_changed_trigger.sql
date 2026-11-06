-- LISTEN/NOTIFY trigger for `launch.launches`, following the pattern in
-- 20260810100002_mailbox_row_change_triggers.sql: the inspect API's
-- `POST /api/launches` fires a launch asynchronously and returns as soon as
-- the ledger row exists, so the rest of a launch's progress (opened,
-- channel-checked, closed) has to reach the SPA and any watching WebSocket
-- client through the same `proxy.row_changed` channel every other live-
-- updating table already uses, not a second bespoke mechanism.
--
-- `id` is the row's own bigint id (mirroring `requests`, not `sessions`,
-- since `launch.launches.id` is the surrogate key a caller polls
-- `GET /api/launches/{session}` around, while `session_uuid` is nullable
-- and not stable identity for a `--resume` launch). `session_id` carries
-- `session_uuid` when the row has one, so a client can filter to "my
-- launch" the same way it already filters `requests` events.

CREATE FUNCTION mike.notify_row_changed_launch_launches()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'launch_launches',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', COALESCE(NEW.session_uuid, OLD.session_uuid)::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE TRIGGER trg_launch_launches_row_changed
AFTER INSERT OR UPDATE OR DELETE ON launch.launches
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_launch_launches();
