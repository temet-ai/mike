SELECT count(*)::bigint
   FROM steering.evaluations e
   JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  WHERE e.steerer_id = 18 AND e.question = 'unobserved_done' AND e.session_id = 1252569 AND e.arm = 'treated'
    AND e.verdict_json -> 'facts' -> 'observation' ->> 'last_change_use_id' = '806624'
    AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL OR i.wake_state = 'pending')
