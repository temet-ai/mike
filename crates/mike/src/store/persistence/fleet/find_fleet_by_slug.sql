SELECT id, fleet_uuid, slug, project_path, state, definition, missing, run_id, start_report,
       created_by, created_at, updated_at, ready_at, started_at, finished_at
  FROM launch.fleets
 WHERE slug = $1
 ORDER BY created_at DESC, id DESC
 LIMIT 1
