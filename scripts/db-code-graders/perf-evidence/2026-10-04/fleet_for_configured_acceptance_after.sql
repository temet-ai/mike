SELECT f.id, f.fleet_uuid, f.slug, f.project_path, f.state, f.definition,
       f.missing, f.run_id, f.start_report, f.created_by, f.created_at,
       f.updated_at, f.ready_at, f.started_at, f.finished_at
  FROM sessions.sessions s
  JOIN projects.projects p ON p.id = s.project_id
  JOIN launch.fleets f ON f.project_path = p.project_path
 WHERE s.session_uuid = '27d90e94-d967-4657-8b40-7556bd0efd51'::uuid
 ORDER BY f.created_at DESC, f.id DESC
