-- Resolve only recorded dispatch relationships; never match a worker by persona.
CREATE OR REPLACE VIEW memory.request_dispatch_parents AS
SELECT r.id AS request_id, u.request_id AS parent_request_id
FROM requests.requests r
JOIN agents.instances i ON i.id = r.agent_instance_id
JOIN agents.dispatches d ON d.id = i.dispatch_id
JOIN tools.uses u ON u.id = d.tool_use_id
UNION ALL
SELECT r.id, u.request_id
FROM agents.instances i
JOIN agents.dispatches d ON d.id = i.dispatch_id
JOIN tools.uses u ON u.id = d.tool_use_id
JOIN requests.requests parent ON parent.id = u.request_id
JOIN requests.requests r ON r.session_id = i.session_id
WHERE i.session_id <> parent.session_id
UNION ALL
SELECT r.id, u.request_id
FROM agents.dispatches d
JOIN tools.uses u ON u.id = d.tool_use_id
JOIN requests.requests parent ON parent.id = u.request_id
JOIN requests.requests r ON r.session_id = parent.session_id
WHERE d.worker_producer_id IS NOT NULL
  AND (jsonb_path_query_first(r.request_headers, 'strict $[*] ? (@[0] == "x-claude-code-agent-id")[1]', '{}'::jsonb, true) #>> '{}') = d.worker_producer_id
UNION ALL
SELECT r.id, spawn.request_id
FROM (
    SELECT u.request_id, result.content
    FROM tools.uses u JOIN tools.results result ON result.tool_use_id = u.id AND NOT result.is_error
    WHERE u.tool_name IN ('spawn_session', 'mcp__mike__spawn_session')
) spawn
-- Captured MCP results may be structured objects, JSON strings or text blocks.
CROSS JOIN LATERAL jsonb_array_elements(
    CASE WHEN jsonb_typeof(spawn.content) = 'array' THEN spawn.content
         ELSE jsonb_build_array(spawn.content) END
) block(value)
CROSS JOIN LATERAL (
    SELECT CASE
        WHEN block.value->>'type' = 'text' AND pg_input_is_valid(block.value->>'text', 'jsonb')
            THEN (block.value->>'text')::jsonb
        WHEN jsonb_typeof(block.value) = 'string' AND pg_input_is_valid(block.value #>> '{}', 'jsonb')
            THEN (block.value #>> '{}')::jsonb
        ELSE block.value END AS value
) payload
JOIN sessions.sessions s ON s.session_uuid::text = payload.value #>> '{launch,session}'
JOIN requests.requests r ON r.session_id = s.id;


CREATE OR REPLACE FUNCTION memory.refresh_inherited_turns(turn_ids bigint[])
RETURNS void LANGUAGE sql AS $$
WITH members AS MATERIALIZED (
    SELECT id, operator_turn_id AS turn_id FROM requests.requests
    WHERE operator_turn_id = ANY(turn_ids)
), tool_totals AS (
    SELECT r.turn_id, count(*) AS calls,
           array_agg(DISTINCT u.tool_name ORDER BY u.tool_name) FILTER (WHERE u.tool_name IS NOT NULL) AS tools,
           array_agg(DISTINCT u.file_path ORDER BY u.file_path) FILTER (WHERE u.file_path IS NOT NULL) AS files,
           array_agg(DISTINCT (u.input->>'skill') ORDER BY (u.input->>'skill'))
               FILTER (WHERE u.tool_name = 'Skill' AND u.input->>'skill' IS NOT NULL) AS skills
    FROM members r JOIN tools.uses u ON u.request_id = r.id GROUP BY r.turn_id
), errors AS (
    SELECT r.turn_id, count(DISTINCT u.id) AS errors
    FROM members r JOIN tools.uses u ON u.request_id = r.id
    JOIN tools.results result ON result.tool_use_id = u.id AND result.is_error GROUP BY r.turn_id
), dispatches AS (
    SELECT r.turn_id, array_agg(DISTINCT a.name ORDER BY a.name) AS agents
    FROM members r JOIN tools.uses u ON u.request_id = r.id
    JOIN agents.dispatches d ON d.tool_use_id = u.id
    JOIN agents.agents a ON a.id = d.agent_id GROUP BY r.turn_id
), findings AS (
    SELECT r.turn_id, count(*) AS findings FROM members r
    JOIN requests.diagnostics d ON d.request_id = r.id AND d.analyzer = 'weasel'
    GROUP BY r.turn_id
), tokens AS (
    SELECT r.turn_id, sum(resp.input_tokens) AS input_tokens, sum(resp.output_tokens) AS output_tokens
    FROM members r JOIN responses.responses resp ON resp.request_id = r.id GROUP BY r.turn_id
), texts AS (
    SELECT r.turn_id, string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS content
    FROM members r JOIN responses.stream_blocks b ON b.request_id = r.id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
    GROUP BY r.turn_id
)
UPDATE memory.operator_turns t SET
    tool_call_count = coalesce(u.calls, 0), tool_error_count = coalesce(e.errors, 0),
    tools_used = coalesce(u.tools, '{}'), files_touched = coalesce(u.files, '{}'),
    skills_used = coalesce(u.skills, '{}'), agents_dispatched = coalesce(d.agents, '{}'),
    weasel_findings_count = coalesce(f.findings, 0),
    total_input_tokens = coalesce(tok.input_tokens, 0), total_output_tokens = coalesce(tok.output_tokens, 0),
    final_assistant_text = texts.content
FROM unnest(turn_ids) ids(id)
LEFT JOIN tool_totals u ON u.turn_id = ids.id
LEFT JOIN errors e ON e.turn_id = ids.id
LEFT JOIN dispatches d ON d.turn_id = ids.id
LEFT JOIN findings f ON f.turn_id = ids.id
LEFT JOIN tokens tok ON tok.turn_id = ids.id
LEFT JOIN texts ON texts.turn_id = ids.id
WHERE t.id = ids.id;
$$;
