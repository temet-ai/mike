SELECT f.id, f.fleet_uuid, f.slug, f.project_path, f.state, f.definition,
       f.missing, f.run_id, f.start_report, f.created_by, f.created_at,
       f.updated_at, f.ready_at, f.started_at, f.finished_at
  FROM launch.launches l
  JOIN launch.fleets f ON f.run_id = l.run_id
 WHERE l.session_uuid = '27d90e94-d967-4657-8b40-7556bd0efd51'::uuid
 ORDER BY l.created_at DESC, l.id DESC, f.id DESC
 LIMIT 1
