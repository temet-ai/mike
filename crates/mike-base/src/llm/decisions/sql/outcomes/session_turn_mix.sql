WITH judged AS (
    SELECT r.id AS record_id, r.session_id,
           r.composed -> 'verdict' ->> 'outcome' AS outcome
      FROM decisions.records r
     WHERE r.point = 'session_judge'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied IS NOT NULL
       AND r.applied <> 'none'
       AND r.session_id IS NOT NULL
),
mix AS (
    SELECT t.session_id,
           count(*) FILTER (WHERE toc.code IN ('accepted', 'shipped')) AS accepted,
           count(*) FILTER (WHERE toc.code = 'corrected') AS corrected
      FROM memory.operator_turns t
      JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
     WHERE t.outcome_source = 'operator'
       AND t.session_id IN (SELECT session_id FROM judged)
     GROUP BY t.session_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT j.record_id,
       CASE j.outcome
           WHEN 'success' THEN CASE WHEN m.accepted > m.corrected THEN 'right' ELSE 'wrong' END
           WHEN 'failure' THEN CASE WHEN m.corrected > m.accepted THEN 'right' ELSE 'wrong' END
           ELSE CASE WHEN m.accepted > 0 AND m.corrected > 0 THEN 'right' ELSE 'wrong' END
       END,
       'operator_turn_mix',
       jsonb_build_object('accepted_turns', m.accepted, 'corrected_turns', m.corrected,
                          'jev_outcome', j.outcome)
  FROM judged j
  JOIN mix m ON m.session_id = j.session_id
 WHERE j.outcome IN ('success', 'partial', 'failure')
   AND m.accepted + m.corrected >= 2
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = j.record_id AND o.label_source = 'operator_turn_mix')
ON CONFLICT (record_id, label_source, label) DO NOTHING
