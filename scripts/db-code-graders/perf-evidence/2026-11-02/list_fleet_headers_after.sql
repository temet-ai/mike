SELECT fleet_uuid, slug, project_path, state, run_id, missing,
       created_at, updated_at, ready_at, started_at, finished_at
  FROM launch.fleets
 WHERE project_path = '/mnt/data/projects/temet/temet-proxy'
 ORDER BY created_at DESC, id DESC
 LIMIT 50
