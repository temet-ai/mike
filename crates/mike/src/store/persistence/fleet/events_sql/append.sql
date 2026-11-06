INSERT INTO launch.fleet_events (run_id, task, session_uuid, kind, payload)
VALUES ($1, $2, $3, $4, $5)
RETURNING id, occurred_at
