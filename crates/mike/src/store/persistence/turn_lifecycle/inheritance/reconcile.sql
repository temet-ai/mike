WITH parent AS MATERIALIZED (
  SELECT DISTINCT u.request_id FROM tools.uses u
  WHERE u.wire_tool_use_id = ANY($1::text[])
    AND u.tool_name IN ('Agent', 'Task', 'spawn_session', 'mcp__mike__spawn_session')
), du AS MATERIALIZED (
  SELECT u.id AS tool_use_id, u.request_id AS parent_request_id, u.tool_name, pr.session_id AS parent_session_id
  FROM parent p JOIN tools.uses u ON u.request_id = p.request_id
  JOIN requests.requests pr ON pr.id = p.request_id
), cand AS (
  SELECT r.id FROM du JOIN agents.dispatches d ON d.tool_use_id = du.tool_use_id
   JOIN agents.instances i ON i.dispatch_id = d.id JOIN requests.requests r ON r.agent_instance_id = i.id AND r.operator_turn_id IS NULL
  UNION ALL
  SELECT r.id FROM du JOIN agents.dispatches d ON d.tool_use_id = du.tool_use_id
   JOIN agents.instances i ON i.dispatch_id = d.id AND i.session_id <> du.parent_session_id
   JOIN requests.requests r ON r.session_id = i.session_id AND r.operator_turn_id IS NULL
  UNION ALL
  SELECT r.id FROM du JOIN agents.dispatches d ON d.tool_use_id = du.tool_use_id AND d.worker_producer_id IS NOT NULL
   JOIN requests.requests r ON r.session_id = du.parent_session_id AND r.claude_code_agent_id = d.worker_producer_id AND r.operator_turn_id IS NULL
  UNION ALL
  SELECT r.id FROM requests.requests r
  WHERE r.operator_turn_id IS NULL
    AND r.session_id = ANY (ARRAY(
      SELECT s.id FROM du
      JOIN tools.results res ON res.tool_use_id = du.tool_use_id AND NOT res.is_error
      CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(res.content) = 'array' THEN res.content ELSE jsonb_build_array(res.content) END) block(value)
      CROSS JOIN LATERAL (SELECT CASE
           WHEN block.value->>'type' = 'text' AND pg_input_is_valid(block.value->>'text', 'jsonb') THEN (block.value->>'text')::jsonb
           WHEN jsonb_typeof(block.value) = 'string' AND pg_input_is_valid(block.value #>> '{}', 'jsonb') THEN (block.value #>> '{}')::jsonb
           ELSE block.value END AS value) payload
      JOIN sessions.sessions s ON s.session_uuid::text = payload.value #>> '{launch,session}'
      WHERE du.tool_name IN ('spawn_session', 'mcp__mike__spawn_session')))
)
SELECT DISTINCT c.id FROM cand c
