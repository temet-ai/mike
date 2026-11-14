WITH answers AS (
    SELECT r.id AS record_id, r.created_at,
           (s ->> 'session_id')::bigint AS session_id,
           (s ->> 'is_probe')::boolean AS is_probe
      FROM decisions.records r
      CROSS JOIN LATERAL jsonb_array_elements(
               COALESCE(r.composed -> 'verdict' -> 'sessions', '[]'::jsonb)
           ) s
     WHERE r.point = 'probe_session'
       AND r.created_at >= now() - interval '30 days'
       AND r.applied = 'chosen'
),
later AS (
    SELECT a.record_id, a.created_at, a.is_probe,
           EXISTS (SELECT 1 FROM memory.operator_turns t
                    WHERE t.session_id = a.session_id
                      AND t.opened_by = 'operator'
                      AND t.created_at > a.created_at) AS operator_turns
      FROM answers a
),
judged AS (
    SELECT l.record_id,
           CASE
               WHEN l.is_probe AND l.operator_turns THEN 'wrong'
               WHEN l.is_probe AND l.created_at <= now() - interval '24 hours' THEN 'right'
               WHEN NOT l.is_probe AND l.operator_turns THEN 'right'
           END AS label
      FROM later l
),
per_record AS (
    SELECT j.record_id,
           CASE WHEN bool_or(j.label = 'wrong') THEN 'wrong' ELSE 'right' END AS label,
           count(*) AS sessions
      FROM judged j
     WHERE j.label IS NOT NULL
     GROUP BY j.record_id
)
INSERT INTO decisions.outcomes (record_id, label, label_source, evidence)
SELECT p.record_id, p.label, 'later_operator_turns',
       jsonb_build_object('judged_sessions', p.sessions)
  FROM per_record p
 WHERE NOT EXISTS (SELECT 1 FROM decisions.outcomes o
                    WHERE o.record_id = p.record_id AND o.label_source = 'later_operator_turns')
ON CONFLICT (record_id, label_source, label) DO NOTHING
