SELECT e.uuid,
       e.operator_turn_id,
       e.status,
       e.intervention_uuid,
       e.dispatched_at,
       e.resolved_at
  FROM steering.evaluations e
  JOIN sessions.sessions s ON s.id = e.session_id
 WHERE s.session_uuid = '5ea98e26-237c-4221-98c1-9c9d079bb557'
 ORDER BY e.dispatched_at DESC
 LIMIT 1;
