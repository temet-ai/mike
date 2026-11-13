WITH attempt AS (
    SELECT source_id
      FROM remote_control.injections
     WHERE event_id = $1 AND source_kind = 'steerer' AND source_id IS NOT NULL
)
UPDATE steering.interventions i
   SET pushed_at = CASE WHEN $2 THEN i.pushed_at ELSE NULL END
  FROM attempt a
 WHERE i.uuid = a.source_id::uuid
