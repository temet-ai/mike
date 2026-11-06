SELECT id, fleet_uuid, slug, project_path, state, definition, missing, run_id, start_report,
       created_by, created_at, updated_at, ready_at, started_at, finished_at
  FROM launch.fleets
 WHERE state = 'done'
   AND definition #>> '{retrospective_job,phase}' IN ('pending', 'launching', 'running', 'cleanup')
 ORDER BY created_at, id
