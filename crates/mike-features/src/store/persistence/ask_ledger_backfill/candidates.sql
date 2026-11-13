SELECT t.id AS turn_id, t.session_id, t.start_request_id,
       COALESCE(t.user_prompt, '') AS "user_prompt!", t.created_at,
       EXISTS (SELECT 1 FROM memory.ask_ledger_entries p
                WHERE p.session_id = t.session_id AND p.created_at < t.created_at) AS "has_earlier!"
  FROM memory.operator_turns t
 WHERE t.session_id = ANY($1)
   AND (t.opened_by IS NULL OR t.opened_by = 'operator')
   AND t.closed_at IS NOT NULL
   AND t.created_at >= $2
   AND NOT EXISTS (SELECT 1 FROM memory.ask_ledger_entries e
                    WHERE e.request_id = t.start_request_id)
 ORDER BY t.session_id, t.created_at, t.id
