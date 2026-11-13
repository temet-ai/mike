INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT r.id, e.outcome_label, 'turn_guard_label',
       jsonb_build_object('evaluation_uuid', e.uuid, 'label_source', e.label_source)
  FROM decisions.records r
  JOIN steering.evaluations e ON e.uuid = r.evaluation_uuid
 WHERE r.point = $1 AND e.outcome_label IS NOT NULL
ON CONFLICT (record_id, label_source, label) DO NOTHING
