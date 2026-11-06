SELECT e.steerer_id, st.name AS steerer_name,
       to_char(least(floor(e.confidence * 20), 19) * 0.05, 'FM0.00') AS lower_edge,
       e.label_source,
       (count(*) FILTER (WHERE e.outcome_label = 'right'))::int AS k,
       count(*)::int AS n
  FROM steering.evaluations e
  JOIN steering.steerers st ON st.id = e.steerer_id
 WHERE st.name = ANY(ARRAY['verifier', 'monitor-over-sleep'])
   AND e.dispatched_at >= '2026-10-14 12:00:00+00'::timestamptz::timestamptz - interval '30 days' AND e.dispatched_at < '2026-10-14 12:00:00+00'::timestamptz
   AND e.outcome_label IN ('right', 'wrong')
   AND e.confidence IS NOT NULL
   AND e.label_source IN ('operator', 'session_reply', 'session_end', 'next_message')
 GROUP BY e.steerer_id, st.name, 3, e.label_source;
