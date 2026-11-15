WITH judged AS (
    SELECT r.id AS record_id, r.created_at, r.session_id,
           r.composed -> 'verdict' ->> 'outcome' AS outcome
      FROM decisions.records r
      JOIN sessions.sessions s ON s.id = r.session_id
     WHERE r.point = 'session_judge'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied IS NOT NULL
       AND r.applied <> 'none'
       AND r.composed -> 'verdict' ->> 'outcome' IN ('success', 'failure')
       AND s.last_seen < now() - interval '2 hours'
),
pushed AS (
    SELECT ce.session_id, min(ce.id) AS push_event_id
      FROM workspace.change_events ce
     WHERE ce.kind = 'push'
       AND ce.at >= now() - interval '60 days'
       AND ce.session_id IN (SELECT session_id FROM judged)
       AND ce.detail ->> 'author' IS DISTINCT FROM 'jev'
     GROUP BY ce.session_id
),
frustrated AS (
    SELECT t.session_id, min(t.id) AS turn_id
      FROM memory.operator_turns t
     WHERE t.outcome_source = 'frustration_close'
       AND t.session_id IN (SELECT session_id FROM judged)
     GROUP BY t.session_id
),
settled AS (
    SELECT j.record_id, j.outcome, p.push_event_id, f.turn_id AS frustration_turn_id,
           CASE
               WHEN j.outcome = 'success' AND f.turn_id IS NOT NULL THEN 'wrong'
               WHEN j.outcome = 'success' AND p.push_event_id IS NOT NULL THEN 'right'
               WHEN j.outcome = 'failure' AND f.turn_id IS NOT NULL THEN 'right'
               WHEN j.outcome = 'failure' AND p.push_event_id IS NOT NULL THEN 'wrong'
           END AS label
      FROM judged j
      LEFT JOIN pushed p ON p.session_id = j.session_id
      LEFT JOIN frustrated f ON f.session_id = j.session_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT s.record_id, s.label, 'session_shipped',
       jsonb_build_object('jev_outcome', s.outcome, 'push_event_id', s.push_event_id,
                          'frustration_turn_id', s.frustration_turn_id)
  FROM settled s
 WHERE s.label IS NOT NULL
   AND NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = s.record_id AND o.label_source = 'session_shipped')
ON CONFLICT (record_id, label_source, label) DO NOTHING
