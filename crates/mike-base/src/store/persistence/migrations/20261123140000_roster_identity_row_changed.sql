-- `proxy.row_changed` notices that let a project roster keep its slow part in
-- memory and a live-peer count answer from memory until something it depends
-- on moves.
--
-- On 2026-11-23 a roster load ran three statements (registrations, channel
-- snapshots, runtime observations), about 1,460 times an hour each across the
-- five machines. The roster now keeps each project's registrations and their
-- Remote Control channel identities in memory and reads them again only after
-- one of these notices; heartbeats, runtime facts and requests are read on each
-- load in one statement.
--
-- workspace_session_registrations: a registration inserted or deleted, or its
-- session, project, cwd, worktree, registered_via, registered_at or released_at
-- changed; or its lease shortened, or renewed after it had lapsed. A renewal of
-- a lease that has not lapsed sends nothing: renewals were about 300 of the
-- table's updates an hour against about 6 registrations or releases, and the
-- roster reads lease_expires_at on every load.
--
-- remote_control_channels: a channel inserted or deleted, or its cse_id,
-- local_session_uuid, session_id or client_version changed; or a linked
-- channel's heartbeat written after its last one had been silent for three
-- heartbeat intervals (at least 20 s each), so a channel coming back to life is
-- seen. Heartbeats inside that window send nothing.
--
-- runtime_session_facts: a facts row inserted or deleted, its lifecycle moving
-- into or out of ended/failed, or its observed_at moving after 14 minutes of
-- silence (the roster's activity window is 15 minutes) or moving back.
--
-- Each notice carries the session's uuid as session_id; it is NULL when the
-- session row is already gone.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_workspace_session_registrations()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
      FROM sessions.sessions s
     WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'workspace_session_registrations',
        'id',         COALESCE(NEW.session_id, OLD.session_id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_session_registrations_row_changed
AFTER INSERT OR DELETE ON workspace.session_registrations
FOR EACH ROW
EXECUTE FUNCTION mike.notify_row_changed_workspace_session_registrations();

CREATE OR REPLACE TRIGGER trg_session_registrations_row_changed_update
AFTER UPDATE OF session_id, project_id, cwd, worktree, registered_via, registered_at,
                released_at, lease_expires_at
ON workspace.session_registrations
FOR EACH ROW
WHEN ((OLD.session_id, OLD.project_id, OLD.cwd, OLD.worktree, OLD.registered_via,
       OLD.registered_at, OLD.released_at)
      IS DISTINCT FROM
      (NEW.session_id, NEW.project_id, NEW.cwd, NEW.worktree, NEW.registered_via,
       NEW.registered_at, NEW.released_at)
      OR NEW.lease_expires_at < OLD.lease_expires_at
      OR (OLD.lease_expires_at <= now() AND NEW.lease_expires_at > now()))
EXECUTE FUNCTION mike.notify_row_changed_workspace_session_registrations();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_remote_control_channels()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'remote_control_channels',
        'id',         COALESCE(NEW.id, OLD.id)::text,
        'session_id', COALESCE(NEW.local_session_uuid, OLD.local_session_uuid)::text,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_rc_channels_row_changed
AFTER INSERT OR DELETE ON remote_control.channels
FOR EACH ROW
EXECUTE FUNCTION mike.notify_row_changed_remote_control_channels();

CREATE OR REPLACE TRIGGER trg_rc_channels_row_changed_update
AFTER UPDATE OF cse_id, local_session_uuid, session_id, client_version,
                last_heartbeat_at, last_heartbeat_interval_seconds
ON remote_control.channels
FOR EACH ROW
WHEN ((OLD.cse_id, OLD.local_session_uuid, OLD.session_id, OLD.client_version)
      IS DISTINCT FROM
      (NEW.cse_id, NEW.local_session_uuid, NEW.session_id, NEW.client_version)
      OR (NEW.local_session_uuid IS NOT NULL
          AND (OLD.last_heartbeat_at, OLD.last_heartbeat_interval_seconds)
              IS DISTINCT FROM
              (NEW.last_heartbeat_at, NEW.last_heartbeat_interval_seconds)
          AND (OLD.last_heartbeat_at IS NULL
               OR OLD.last_heartbeat_at < now() - make_interval(
                      secs => 3 * GREATEST(COALESCE(OLD.last_heartbeat_interval_seconds, 20), 20) - 2))))
EXECUTE FUNCTION mike.notify_row_changed_remote_control_channels();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_runtime_session_facts()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    sess_uuid text;
BEGIN
    SELECT s.session_uuid::text INTO sess_uuid
      FROM sessions.sessions s
     WHERE s.id = COALESCE(NEW.session_id, OLD.session_id);
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'runtime_session_facts',
        'id',         COALESCE(NEW.session_id, OLD.session_id)::text,
        'session_id', sess_uuid,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_session_facts_row_changed
AFTER INSERT OR DELETE ON runtime.session_facts
FOR EACH ROW
EXECUTE FUNCTION mike.notify_row_changed_runtime_session_facts();

CREATE OR REPLACE TRIGGER trg_session_facts_row_changed_update
AFTER UPDATE OF session_id, lifecycle, observed_at ON runtime.session_facts
FOR EACH ROW
WHEN (OLD.session_id IS DISTINCT FROM NEW.session_id
      OR (COALESCE(OLD.lifecycle, '') IN ('ended', 'failed'))
         IS DISTINCT FROM (COALESCE(NEW.lifecycle, '') IN ('ended', 'failed'))
      OR NEW.observed_at < OLD.observed_at
      OR (NEW.observed_at > OLD.observed_at
          AND OLD.observed_at < now() - interval '14 minutes'))
EXECUTE FUNCTION mike.notify_row_changed_runtime_session_facts();
