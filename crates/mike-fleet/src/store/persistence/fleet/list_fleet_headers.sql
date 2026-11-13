SELECT fleet_uuid, slug, project_path, state, run_id, missing,
       created_at, updated_at, ready_at, started_at, finished_at
  FROM launch.fleets
 ORDER BY created_at DESC, id DESC
 LIMIT $1
