SELECT f.id, f.fleet_uuid, f.slug, f.project_path, f.state, f.definition,
       f.missing, f.run_id, f.start_report, f.created_by, f.created_at,
       f.updated_at, f.ready_at, f.started_at, f.finished_at
  FROM launch.launches l
  JOIN launch.fleets f ON f.run_id = l.run_id
 WHERE l.session_uuid = '4fe05978-2d4e-4f99-bce0-454578cb3224'::uuid
 ORDER BY l.created_at DESC, l.id DESC, f.id DESC
 LIMIT 1
