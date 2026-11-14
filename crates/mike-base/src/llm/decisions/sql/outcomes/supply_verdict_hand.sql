INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT r.id,
       CASE WHEN v.hand_verdict = r.composed -> 'verdict' ->> 'verdict' THEN 'right' ELSE 'wrong' END,
       'hand_verdict',
       jsonb_build_object('supplied_item_id', v.supplied_item_id,
                          'hand_verdict', v.hand_verdict,
                          'jev_verdict', r.composed -> 'verdict' ->> 'verdict',
                          'checked_by', v.hand_checked_by)
  FROM decisions.records r
  JOIN memory.supplied_item_verdicts v
    ON v.supplied_item_id = (r.composed -> 'verdict' ->> 'item_id')::bigint
 WHERE r.point = 'supply_verdict'
   AND r.created_at >= now() - interval '30 days'
   AND r.applied = 'labelled'
   AND r.composed -> 'verdict' ->> 'verdict' IS NOT NULL
   AND v.hand_verdict IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = r.id AND o.label_source = 'hand_verdict')
ON CONFLICT (record_id, label_source, label) DO NOTHING
