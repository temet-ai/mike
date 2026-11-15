INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT DISTINCT ON (r.id)
       r.id,
       CASE WHEN (e.outcome_label = 'right') = (r.composed -> 'verdict' ->> 'fires' = 'true')
            THEN 'right' ELSE 'wrong' END,
       'steerer_nudge_reply',
       jsonb_build_object('evaluation_uuid', e.uuid,
                          'nudge_label', e.outcome_label,
                          'jev_fires', r.composed -> 'verdict' ->> 'fires')
  FROM decisions.records r
  JOIN steering.evaluations e
    ON e.operator_turn_id = r.operator_turn_id AND e.trigger_request_id = r.request_id
  LEFT JOIN decisions.outcomes done
    ON done.record_id = r.id AND done.label_source = 'steerer_nudge_reply'
 WHERE r.point = 'steerer_checklist'
   AND done.record_id IS NULL
   AND e.mode = 'live'
   AND e.intervention_uuid IS NOT NULL
   AND e.label_source = 'session_reply'
   AND e.outcome_label IN ('right', 'wrong')
   AND e.verdict_json ->> 'fires' = 'true'
   AND r.composed -> 'verdict' ->> 'fires' IN ('true', 'false')
 ORDER BY r.id, e.id
ON CONFLICT (record_id, label_source, label) DO NOTHING
