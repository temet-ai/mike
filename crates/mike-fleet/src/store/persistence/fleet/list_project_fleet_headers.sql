SELECT fleet_uuid, slug, project_path, state, run_id, missing,
       created_at, updated_at, ready_at, started_at, finished_at
  FROM launch.fleets
 WHERE project_path = $1
 ORDER BY created_at DESC, id DESC
 LIMIT $2
