WITH run AS (
    SELECT session_uuid FROM wright.investigations WHERE id = $1
),
s AS (
    SELECT id FROM sessions.sessions WHERE session_uuid = (SELECT session_uuid FROM run)
),
calls AS (
    SELECT count(*)::integer AS n FROM tools.uses u WHERE u.session_id IN (SELECT id FROM s)
),
spent AS (
    SELECT COALESCE(sum(COALESCE(p.input_tokens, 0) + COALESCE(p.output_tokens, 0)
                        + COALESCE(p.cache_creation_input_tokens, 0)), 0)::bigint AS n
      FROM requests.requests r
      JOIN responses.responses p ON p.request_id = r.id
     WHERE r.session_id IN (SELECT id FROM s)
)
UPDATE wright.investigations i
   SET tool_calls = calls.n, tokens = spent.n
  FROM calls, spent, wright.engineer_limits l
 WHERE i.id = $1 AND l.id
RETURNING i.tool_calls, i.tokens, i.deadline_at, i.state,
          l.tool_calls AS max_tool_calls, l.tokens AS max_tokens
