SELECT i.steerer_id, s.name AS steerer_name,
       i.payload_text, i.created_at,
       CASE WHEN e.labelled_at <= now() THEN e.outcome_label END AS outcome_label,
       CASE WHEN e.labelled_at <= now() THEN e.label_evidence->>'reaction' END AS reaction
  FROM steering.interventions i
  JOIN steering.verdicts v ON v.id = i.verdict_id AND v.code = 'fired'
  JOIN steering.steerers s ON s.id = i.steerer_id
  LEFT JOIN steering.evaluations e ON e.intervention_uuid = i.uuid
 WHERE i.session_id = 1504120
   AND i.created_at >= now() - interval '24 hours'
   AND i.created_at <= now()
   AND (i.reverted_at IS NULL OR i.reverted_at > now())
   AND (i.wake_state IS NULL
        OR i.wake_state IN ('pending', 'delivering', 'pushed', 'appended'))
   AND (i.delivered_request_id IS NOT NULL
        OR i.pushed_at IS NOT NULL
        OR i.expires_at > now())
 ORDER BY i.created_at DESC
 LIMIT 40
