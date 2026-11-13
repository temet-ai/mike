INSERT INTO launch.fleets (slug, project_path, definition, missing, created_by)
VALUES ($1, $2, $3, $4, $5)
RETURNING id, fleet_uuid, slug, project_path, state, definition, missing, run_id, start_report,
       created_by, created_at, updated_at, ready_at, started_at, finished_at
