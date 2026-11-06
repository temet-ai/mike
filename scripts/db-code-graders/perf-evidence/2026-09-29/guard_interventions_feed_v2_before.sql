SELECT e.uuid, s.session_uuid, e.session_id, e.operator_turn_id, e.question, e.arm,
       e.dispatched_at, e.outcome_label, e.label_source,
       i.delivered_request_id, i.wake_state,
       rx.reaction AS outcome_reaction
  FROM steering.evaluations e
  JOIN sessions.sessions s ON s.id = e.session_id
  JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  LEFT JOIN LATERAL (
      SELECT v.reaction
        FROM memory.turn_verdicts v
       WHERE v.operator_turn_id = e.operator_turn_id AND v.mode = 'live'
       ORDER BY v.created_at DESC, v.id DESC
       LIMIT 1
  ) rx ON true
 WHERE e.steerer_id = 18
   AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL)
 ORDER BY e.dispatched_at DESC, e.id DESC
 LIMIT 50 OFFSET 0;
