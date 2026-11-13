SELECT r.id AS request_id,
       r.session_id,
       r.created_at,
       o.code AS origin,
       tt.code AS turn_type,
       ak.code AS author,
       (r.claude_code_agent_id IS NOT NULL) AS "has_agent_id!",
       r.human_prompt,
       r.last_user_message,
       (ts.id IS NOT NULL) AS "opened!",
       (c.request_id IS NOT NULL) AS "closed_turn!",
       (r.operator_turn_id IS NOT NULL) AS "attached!"
  FROM requests.requests r
  JOIN requests.turn_types tt ON tt.id = r.turn_type_id
  JOIN requests.turn_origins o ON o.id = r.turn_origin_id
  JOIN requests.author_kinds ak ON ak.id = r.author_kind_id
  LEFT JOIN memory.operator_turns ts ON ts.start_request_id = r.id
  LEFT JOIN (SELECT DISTINCT outcome_evidence_request_id AS request_id
               FROM memory.operator_turns
              WHERE created_at >= $1 AND created_at < $2) c ON c.request_id = r.id
 WHERE r.created_at >= $1 AND r.created_at < $2
   AND o.code IN ('primary', 'primary_first')
 ORDER BY r.id
