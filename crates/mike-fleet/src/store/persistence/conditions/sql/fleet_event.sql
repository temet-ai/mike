SELECT EXISTS (
    SELECT 1
      FROM launch.fleet_events
     WHERE run_id = $1
       AND kind = $2
       AND ($3::text IS NULL OR task = $3)
       AND ($4::uuid IS NULL OR session_uuid = $4)
       AND occurred_at >= $5
) AS "holds!"
