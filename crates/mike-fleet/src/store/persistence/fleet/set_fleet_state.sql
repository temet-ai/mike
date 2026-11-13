UPDATE launch.fleets
   SET state        = $2,
       run_id       = COALESCE($3, run_id),
       start_report = COALESCE($4, start_report),
       ready_at     = CASE WHEN $2 = 'ready' THEN now() ELSE ready_at END,
       started_at   = CASE WHEN $2 = 'running' THEN now() ELSE started_at END,
       finished_at  = CASE WHEN $2 IN ('failed', 'done') THEN now() ELSE finished_at END,
       updated_at   = now()
 WHERE fleet_uuid = $1
RETURNING id, fleet_uuid, slug, project_path, state, definition, missing, run_id, start_report,
       created_by, created_at, updated_at, ready_at, started_at, finished_at
