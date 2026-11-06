SELECT e.uuid, e.steerer_id, s.name AS steerer_name, s.prompt AS steerer_rule, e.session_id,
       e.operator_turn_id, e.question, i.payload_text, i.trigger_request_id,
       i.delivered_request_id, r.operator_turn_id AS reply_turn_id,
       (i.reverted_at IS NOT NULL) AS reverted,
       i.revert_reason
  FROM steering.evaluations e
  JOIN steering.interventions i ON i.uuid = e.intervention_uuid
  JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
  JOIN steering.steerers s ON s.id = e.steerer_id
  LEFT JOIN requests.requests r ON r.id = i.delivered_request_id
  LEFT JOIN memory.operator_turns rt ON rt.id = r.operator_turn_id
 WHERE e.intervention_uuid IS NOT NULL
   AND (e.outcome_label IS NULL
        OR (e.label_source = 'session_reply'
            AND e.label_evidence->>'labeller_version' IS DISTINCT FROM 'reply-labeller-v2'
            AND e.dispatched_at >= now() - interval '72 hours'))
   AND e.status <> 'dispatched'
   AND e.dispatched_at >= now() - interval '14 days'
   AND ((i.delivered_request_id IS NOT NULL AND r.completed_at IS NOT NULL
         AND (rt.closed_at IS NOT NULL
              OR r.completed_at < now() - make_interval(secs => 600)))
        OR (i.delivered_request_id IS NULL
            AND (i.reverted_at IS NOT NULL OR i.expires_at < now())))
 ORDER BY e.dispatched_at, e.id
 LIMIT 20
