SELECT f.fleet_uuid, f.slug, f.project_path, f.state, f.run_id, f.missing,
       f.created_at, f.updated_at, f.ready_at, f.started_at, f.finished_at
  FROM sessions.sessions s
  JOIN projects.projects p ON p.id = s.project_id
  JOIN launch.fleets f ON f.project_path = p.project_path
 WHERE s.session_uuid = $1
   AND lower(btrim(f.acceptance_session)) = $1::text
 ORDER BY f.created_at DESC, f.id DESC
 LIMIT 1
