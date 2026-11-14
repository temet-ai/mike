WITH linked AS (
    SELECT r.id AS record_id, r.operator_turn_id AS turn_id
      FROM decisions.records r
     WHERE r.point = ANY($1)
       AND r.created_at >= now() - interval '30 days'
       AND r.applied IS NOT NULL
       AND r.applied <> 'none'
       AND ($2 = false OR r.applied = 'warned')
       AND r.operator_turn_id IS NOT NULL
    UNION ALL
    SELECT r.id, t.id
      FROM decisions.records r
      JOIN requests.requests rq ON rq.id = r.request_id
      JOIN memory.operator_turns t
        ON t.session_id = rq.session_id
       AND r.request_id BETWEEN t.start_request_id AND t.end_request_id
     WHERE r.point = ANY($1)
       AND r.created_at >= now() - interval '30 days'
       AND r.applied IS NOT NULL
       AND r.applied <> 'none'
       AND ($2 = false OR r.applied = 'warned')
       AND r.operator_turn_id IS NULL
       AND r.request_id IS NOT NULL
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT DISTINCT ON (l.record_id)
       l.record_id,
       CASE WHEN (toc.code = 'corrected') = $2 THEN 'right' ELSE 'wrong' END,
       $3,
       jsonb_build_object('operator_turn_id', t.id,
                          'turn_outcome', toc.code,
                          'outcome_source', t.outcome_source)
  FROM linked l
  JOIN memory.operator_turns t ON t.id = l.turn_id
  JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
 WHERE toc.code IN ('accepted', 'shipped', 'corrected')
   AND t.outcome_source IN ('operator', 'judge')
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = l.record_id AND o.label_source = $3)
 ORDER BY l.record_id, t.id
ON CONFLICT (record_id, label_source, label) DO NOTHING
