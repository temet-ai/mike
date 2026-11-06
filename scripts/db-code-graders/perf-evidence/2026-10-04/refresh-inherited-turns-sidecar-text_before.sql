WITH members AS MATERIALIZED (
    SELECT id, operator_turn_id AS turn_id FROM requests.requests
    WHERE operator_turn_id = ANY(ARRAY[24517,24518,24519,24520,24521,24522,24523,24524,24525,24526,24527,24528,24529,24530,24531,24532,24533,24534,24535,24536,24537,24538,24539,24540,24541,24542,24543,24544,24545,24546,24547,24548,24549,24550,24551,24552,24553,24554,24555,24556,24557,24558,24559,24560,24561,24562,24563,24564,24565,24566]::bigint[])
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
FROM unnest(ARRAY[24517,24518,24519,24520,24521,24522,24523,24524,24525,24526,24527,24528,24529,24530,24531,24532,24533,24534,24535,24536,24537,24538,24539,24540,24541,24542,24543,24544,24545,24546,24547,24548,24549,24550,24551,24552,24553,24554,24555,24556,24557,24558,24559,24560,24561,24562,24563,24564,24565,24566]::bigint[]) ids(id)
LEFT JOIN tool_totals u ON u.turn_id = ids.id
LEFT JOIN errors e ON e.turn_id = ids.id
LEFT JOIN dispatches d ON d.turn_id = ids.id
LEFT JOIN findings f ON f.turn_id = ids.id
LEFT JOIN tokens tok ON tok.turn_id = ids.id
LEFT JOIN texts ON texts.turn_id = ids.id
WHERE t.id = ids.id;
;
