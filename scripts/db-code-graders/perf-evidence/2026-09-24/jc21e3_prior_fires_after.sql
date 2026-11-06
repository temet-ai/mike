SELECT i.id AS intervention_id, i.operator_turn_id, i.payload_text,
       i.missing_evidence, i.off_thread_topic, i.created_at
  FROM steering.interventions i
 WHERE i.session_id = 634160
   AND i.steerer_id = 6
   AND i.created_at < '2026-09-23 08:00:00+00'
   AND i.trigger_request_id IS DISTINCT FROM 429105
 ORDER BY i.created_at DESC, i.id DESC
 LIMIT 10;
