SELECT t.id AS "id!"
  FROM memory.operator_turns t
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  JOIN requests.requests r ON r.id = t.start_request_id
 WHERE t.closed_at >= $1 AND t.closed_at < $2
   AND (f.prompt_kind <> 'genuine' OR f.session_class = 'synthetic'
        OR r.claude_code_agent_id IS NOT NULL)
   AND (t.outcome_source IS NULL OR t.outcome_projection_version IS DISTINCT FROM $5::smallint)
   AND t.id > $3
 ORDER BY t.id
 LIMIT $4
