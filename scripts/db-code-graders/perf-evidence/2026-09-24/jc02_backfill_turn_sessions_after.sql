SELECT DISTINCT t.session_id
   FROM memory.operator_turns t
  WHERE t.created_at >= '2026-07-27 00:00:00+00'
    AND t.created_at < '2026-09-24 16:05:00+00'
  ORDER BY t.session_id
