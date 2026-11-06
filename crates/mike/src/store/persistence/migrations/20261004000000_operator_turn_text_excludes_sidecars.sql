-- A turn's final_assistant_text is what the assistant said in the turn's own
-- conversation. Sidecar requests (next-input suggestions, titles, probes) are
-- stamped with the operator turn but their text is not the assistant's answer;
-- a suggestion such as "switch it off, local only for now" read as if the
-- operator had said it. Closed turns of the last 14 days are recomputed without sidecar text.

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
    FROM members r
    JOIN requests.requests rq ON rq.id = r.id
    JOIN requests.turn_origins o ON o.id = rq.turn_origin_id AND o.code NOT LIKE '%sidecar'
    JOIN responses.stream_blocks b ON b.request_id = r.id
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

WITH affected AS (
    SELECT DISTINCT r.operator_turn_id AS turn_id
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code LIKE '%sidecar'
    JOIN responses.stream_blocks b ON b.request_id = r.id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
    WHERE r.operator_turn_id IS NOT NULL
      AND r.created_at > now() - interval '14 days'
), texts AS (
    SELECT a.turn_id, string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS content
    FROM affected a
    JOIN requests.requests r ON r.operator_turn_id = a.turn_id
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id AND o.code NOT LIKE '%sidecar'
    JOIN responses.stream_blocks b ON b.request_id = r.id
    JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
    GROUP BY a.turn_id
)
UPDATE memory.operator_turns t
   SET final_assistant_text = x.content
  FROM (SELECT a.turn_id, tx.content
          FROM affected a LEFT JOIN texts tx ON tx.turn_id = a.turn_id) x
 WHERE t.id = x.turn_id
   AND t.closed_at IS NOT NULL
   AND t.final_assistant_text IS DISTINCT FROM x.content;
