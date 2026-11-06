WITH attempt AS (
    SELECT source_id
      FROM remote_control.injections
     WHERE event_id = '44444444-4444-4444-4444-444444444444' AND source_kind = 'steerer' AND source_id IS NOT NULL
)
UPDATE steering.interventions i
   SET pushed_at = CASE WHEN true THEN i.pushed_at ELSE NULL END
  FROM attempt a
 WHERE i.uuid = a.source_id::uuid
