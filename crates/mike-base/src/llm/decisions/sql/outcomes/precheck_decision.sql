INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT r.id,
       CASE WHEN (r.composed -> 'verdict' ->> 'send' = 'true') = (e.outcome_label = 'right')
            THEN 'right' ELSE 'wrong' END,
       'precheck_decision',
       jsonb_build_object('evaluation_uuid', e.uuid,
                          'send', r.composed -> 'verdict' ->> 'send',
                          'fire_label', e.outcome_label,
                          'fire_label_source', e.label_source)
  FROM decisions.records r
  JOIN steering.evaluations e ON e.uuid = r.evaluation_uuid
  LEFT JOIN decisions.outcomes done
    ON done.record_id = r.id AND done.label_source = 'precheck_decision'
 WHERE r.point = ANY($1)
   AND done.record_id IS NULL
   AND e.outcome_label IN ('right', 'wrong')
   AND r.composed -> 'verdict' ->> 'send' IN ('true', 'false')
ON CONFLICT (record_id, label_source, label) DO NOTHING
