-- Two feed tables, so readers of checkouts keep their answers in memory and
-- read again only when the rows behind them change.
--
-- `locations`: a `projects.locations` row is added, removed, or one of the
-- columns the checkout readers use changes (project, machine, root, origin,
-- parent, created_at, or present_at turning NULL or non-NULL), or a project's
-- merge target or uuid changes. The knowledge scan read this machine's
-- locations two or three times on every 120 s tick (`locations_on_machine`,
-- `checkout_rows`; about 288 reads an hour across the five machines on
-- 2026-11-23), and folder placement read them on every request that placed a
-- folder. Locations change a few times a day. The scan refreshes `present_at`
-- at most every 10 minutes while a checkout is on disk; that refresh and the
-- `last_seen` bumps of re-registration and knowledge writes send nothing. The
-- payload names the location's machine, so one pass over a machine's
-- locations sends one notice.
--
-- `checkout_edits`: a session's Edit, Write, MultiEdit or NotebookEdit tool
-- call on an absolute path is captured. The change-record supervisor watches
-- every checkout a session edited in the last 30 minutes, including folders
-- outside the session's own working folder (a worktree, another repository);
-- this notice tells it a new folder may have joined. Tool uses are written in
-- the request's own transaction, which already notifies, and the payload is
-- constant, so a request with many edits sends one notice.
--
-- The `tools.uses` trigger is created last so this migration takes the busy
-- table's lock after the others, in the order request writes take them.

CREATE OR REPLACE FUNCTION mike.notify_row_changed_locations()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'locations',
        'id',         COALESCE(NEW.machine_uuid, OLD.machine_uuid)::text,
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_locations_row_changed
AFTER INSERT OR DELETE ON projects.locations
FOR EACH ROW EXECUTE FUNCTION mike.notify_row_changed_locations();

CREATE OR REPLACE TRIGGER trg_locations_row_changed_update
AFTER UPDATE OF project_id, machine_uuid, root_path, origin_url, parent_location_uuid,
                present_at, created_at ON projects.locations
FOR EACH ROW
WHEN ((OLD.project_id, OLD.machine_uuid, OLD.root_path, OLD.origin_url,
       OLD.parent_location_uuid, OLD.present_at IS NULL, OLD.created_at)
      IS DISTINCT FROM
      (NEW.project_id, NEW.machine_uuid, NEW.root_path, NEW.origin_url,
       NEW.parent_location_uuid, NEW.present_at IS NULL, NEW.created_at))
EXECUTE FUNCTION mike.notify_row_changed_locations();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_project_locations()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'locations',
        'id',         'projects.projects',
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_projects_locations_row_changed
AFTER UPDATE OF merged_into_id, project_uuid ON projects.projects
FOR EACH ROW
WHEN ((OLD.merged_into_id, OLD.project_uuid) IS DISTINCT FROM (NEW.merged_into_id, NEW.project_uuid))
EXECUTE FUNCTION mike.notify_row_changed_project_locations();

CREATE OR REPLACE FUNCTION mike.notify_row_changed_checkout_edits()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('proxy.row_changed', json_build_object(
        'table',      'checkout_edits',
        'id',         'tools.uses',
        'session_id', NULL,
        'op',         TG_OP
    )::text);
    RETURN NULL;
END; $$;

CREATE OR REPLACE TRIGGER trg_tool_uses_checkout_edits
AFTER INSERT ON tools.uses
FOR EACH ROW
WHEN (NEW.tool_name IN ('Edit', 'Write', 'MultiEdit', 'NotebookEdit')
      AND starts_with(NEW.file_path, '/'))
EXECUTE FUNCTION mike.notify_row_changed_checkout_edits();
