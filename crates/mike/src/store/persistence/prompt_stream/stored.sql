SELECT c.session_id,
       r.id AS request_id,
       r.created_at,
       t.id AS "turn_id?",
       k.code AS "kind_code?",
       c.rule,
       c.repeat_key,
       c.classifier_version,
       NULL::text AS "text?",
       NULL::text AS "author?",
       (r.claude_code_agent_id IS NOT NULL) AS "has_agent_id!"
  FROM prompt_kinds.classifications c
  JOIN prompt_kinds.kinds k ON k.id = c.kind_id
  JOIN requests.requests r ON r.id = c.request_id
  LEFT JOIN memory.operator_turns t ON t.start_request_id = r.id
 WHERE c.session_id = ANY($1)
   AND r.created_at >= $2
   AND r.created_at <= $3
   AND k.code <> 'empty-prompt'
 ORDER BY c.session_id, r.id
