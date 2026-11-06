SELECT s.id, s.name, GREATEST(s.updated_at, now() - interval '72 hours') AS window_start,
       count(e.id) FILTER (WHERE e.outcome_label = 'right') AS right_n,
       count(e.id) FILTER (WHERE e.outcome_label = 'wrong') AS wrong_n
  FROM steering.steerers s
  LEFT JOIN steering.evaluations e
         ON e.steerer_id = s.id AND e.intervention_uuid IS NOT NULL
        AND e.label_source IN ('session_reply', 'operator')
        AND e.dispatched_at >= GREATEST(s.updated_at, now() - interval '72 hours')
 WHERE s.enabled AND s.deleted_at IS NULL AND s.kind <> 'turn_guard'
 GROUP BY s.id, s.name, s.updated_at ORDER BY s.id;
