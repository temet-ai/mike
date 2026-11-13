UPDATE launch.fleets
   SET state = 'done',
       definition = COALESCE($4, definition),
       finished_at = now(),
       updated_at = now()
 WHERE fleet_uuid = $1
   AND state = $2
   AND run_id IS NOT DISTINCT FROM $3
RETURNING id, fleet_uuid, slug, project_path, state, definition, missing, run_id, start_report,
          created_by, created_at, updated_at, ready_at, started_at, finished_at
