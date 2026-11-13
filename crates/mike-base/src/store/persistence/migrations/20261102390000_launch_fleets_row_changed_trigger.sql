-- `proxy.row_changed` for `launch.fleets`, so the change feed knows when a
-- fleet's definition, state or run changes. Per-session fleet lookups
-- (`find_fleet_for_session`) are served from memory until a launch of that
-- session, the session row or any fleet changes; without this notification a
-- fleet saved by the other machine would stay invisible to that memo.
--
-- Fleets are written a handful of times per run, so the commit-time NOTIFY
-- lock this adds is negligible. `session_id` is NULL: a fleet is not
-- session-scoped, so consumers treat it as a table-wide change. UPDATE
-- notifies only when a column changed.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_launch_fleets()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'launch_fleets',
        'id',         COALESCE(NEW.fleet_uuid, OLD.fleet_uuid)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_launch_fleets_row_changed
AFTER INSERT OR DELETE ON launch.fleets
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_launch_fleets();

CREATE OR REPLACE TRIGGER trg_launch_fleets_row_changed_update
AFTER UPDATE ON launch.fleets
FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*)
EXECUTE FUNCTION mike.notify_row_changed_launch_fleets();
