SELECT count(*) FROM steering.evaluations e
   JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  WHERE e.mode = 'live' AND e.status = 'resolved_correction'
    AND i.delivered_request_id IS NOT NULL
    AND e.outcome_label IS NULL;
