SELECT
       count(DISTINCT (e.operator_turn_id, e.question))
         FILTER (WHERE e.outcome_label IS NULL AND rx.reaction = ANY(ARRAY['accepted','redirected','corrected','closed_in_frustration','no_response','unclear'])) AS unlabelled,
       count(*) FILTER (WHERE i.delivery_channel = 'wake'
                          AND e.dispatched_at >= NOW() - interval '7 days') AS wakes_7d,
       count(*) FILTER (WHERE i.delivery_channel = 'append'
                          AND e.dispatched_at >= NOW() - interval '7 days') AS appends_7d
  FROM steering.evaluations e
  JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  LEFT JOIN LATERAL (
      SELECT v.reaction
        FROM memory.turn_verdicts v
       WHERE e.outcome_label IS NULL
         AND v.operator_turn_id = e.operator_turn_id AND v.mode = 'live'
       ORDER BY v.created_at DESC, v.id DESC
       LIMIT 1
  ) rx ON true
 WHERE e.steerer_id = 18
   AND (i.delivered_request_id IS NOT NULL OR i.pushed_at IS NOT NULL);
