SELECT t.id AS turn_id, t.session_id, t.start_request_id,
       COALESCE(t.user_prompt, '') AS "user_prompt!", r.created_at AS "created_at!"
  FROM memory.operator_turns t
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id AND f.prompt_kind = 'genuine'
  JOIN requests.requests r ON r.id = t.start_request_id
 WHERE t.session_id = ANY($1)
   AND (t.opened_by IS NULL OR t.opened_by = 'operator')
 ORDER BY t.session_id, r.created_at, t.id
