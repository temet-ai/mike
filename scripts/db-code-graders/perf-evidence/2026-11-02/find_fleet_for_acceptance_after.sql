SELECT f.id, f.fleet_uuid, f.slug, f.project_path, f.state, f.definition,
       f.missing, f.run_id, f.start_report, f.created_by, f.created_at,
       f.updated_at, f.ready_at, f.started_at, f.finished_at
  FROM sessions.sessions s
  JOIN projects.projects p ON p.id = s.project_id
  JOIN launch.fleets f ON f.project_path = p.project_path
 WHERE s.session_uuid = 'b4ceb260-1ad7-4ba0-b350-f02f19b7e888'::uuid
   AND lower(btrim(f.acceptance_session)) = 'b4ceb260-1ad7-4ba0-b350-f02f19b7e888'::uuid::text
 ORDER BY f.created_at DESC, f.id DESC
 LIMIT 1
