INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT r.id,
       CASE WHEN e.kind = r.composed -> 'verdict' ->> 'kind' THEN 'right' ELSE 'wrong' END,
       'operator_kind',
       jsonb_build_object('ask_entry_id', e.id,
                          'operator_kind', e.kind,
                          'jev_kind', r.composed -> 'verdict' ->> 'kind')
  FROM decisions.records r
  JOIN memory.ask_ledger_entries e ON e.request_id = r.request_id
 WHERE r.point = 'correction_classifier'
   AND r.applied IN ('warned', 'passed')
   AND e.kind_source = 'operator'
ON CONFLICT (record_id, label_source, label) DO NOTHING
