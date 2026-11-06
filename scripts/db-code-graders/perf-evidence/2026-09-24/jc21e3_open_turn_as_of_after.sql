SELECT t.user_prompt
  FROM memory.operator_turns t
 WHERE t.session_id = 1173418
   AND t.created_at <= '2026-09-24 04:36:00+00'
   AND (t.closed_at IS NULL OR t.closed_at > '2026-09-24 04:36:00+00')
 ORDER BY t.id DESC
 LIMIT 1;
