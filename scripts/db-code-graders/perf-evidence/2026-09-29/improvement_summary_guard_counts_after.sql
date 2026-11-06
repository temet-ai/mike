SELECT
       count(*) FILTER (WHERE e.outcome_label IS NULL) AS unlabelled,
       count(*) FILTER (WHERE i.delivery_channel = 'wake'
                          AND e.dispatched_at >= NOW() - interval '7 days') AS wakes_7d,
       count(*) FILTER (WHERE i.delivery_channel = 'append'
                          AND e.dispatched_at >= NOW() - interval '7 days') AS appends_7d
  FROM steering.evaluations e
  JOIN steering.interventions i ON i.uuid = e.intervention_uuid
 WHERE e.steerer_id = 18
   AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL);
