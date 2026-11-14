INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT r.id,
       CASE WHEN toc.code = r.composed -> 'verdict' ->> 'label' THEN 'right' ELSE 'wrong' END,
       'operator_label',
       jsonb_build_object('operator_turn_id', t.id,
                          'operator_outcome', toc.code,
                          'jev_label', r.composed -> 'verdict' ->> 'label')
  FROM decisions.records r
  JOIN memory.operator_turns t ON t.id = r.operator_turn_id
  JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
 WHERE r.point = 'turn_judge'
   AND r.applied = 'sent'
   AND t.outcome_source = 'operator'
ON CONFLICT (record_id, label_source, label) DO NOTHING
