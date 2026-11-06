WITH members AS MATERIALIZED (
    SELECT r.id, o.code NOT LIKE '%sidecar' AS conversation
      FROM memory.operator_turns t
      JOIN requests.requests r ON r.operator_turn_id = t.id
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE t.id = $1
       AND t.closed_at IS NOT NULL
), tool_totals AS (
    SELECT count(*) AS calls,
           array_agg(DISTINCT u.tool_name ORDER BY u.tool_name) AS tools,
           array_agg(DISTINCT u.file_path ORDER BY u.file_path)
               FILTER (WHERE u.file_path IS NOT NULL) AS files,
           array_agg(DISTINCT (u.input ->> 'skill') ORDER BY (u.input ->> 'skill'))
               FILTER (WHERE u.tool_name = 'Skill' AND u.input ->> 'skill' IS NOT NULL) AS skills
      FROM members m
      JOIN tools.uses u ON u.request_id = m.id
), errors AS (
    SELECT count(DISTINCT u.id) AS errors
      FROM members m
      JOIN tools.uses u ON u.request_id = m.id
      JOIN tools.results res ON res.tool_use_id = u.id AND res.is_error
), dispatched AS (
    SELECT array_agg(DISTINCT ag.name ORDER BY ag.name) AS agents
      FROM members m
      JOIN tools.uses u ON u.request_id = m.id
      JOIN agents.dispatches ad ON ad.tool_use_id = u.id
      JOIN agents.agents ag ON ag.id = ad.agent_id
), findings AS (
    SELECT count(*) AS findings
      FROM members m
      JOIN requests.diagnostics d ON d.request_id = m.id AND d.analyzer = 'weasel'
), tokens AS (
    SELECT sum(resp.input_tokens) AS input_tokens, sum(resp.output_tokens) AS output_tokens
      FROM members m
      JOIN responses.responses resp ON resp.request_id = m.id
), texts AS (
    SELECT string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord) AS content
      FROM members m
      JOIN responses.stream_blocks b ON b.request_id = m.id
      JOIN responses.stream_block_types bt ON bt.id = b.block_type_id AND bt.code = 'text'
     WHERE m.conversation
)
UPDATE memory.operator_turns t
   SET tool_call_count = tt.calls,
       tool_error_count = e.errors,
       tools_used = COALESCE(tt.tools, '{}'),
       files_touched = COALESCE(tt.files, '{}'),
       skills_used = COALESCE(tt.skills, '{}'),
       agents_dispatched = COALESCE(d.agents, '{}'),
       weasel_findings_count = f.findings,
       total_input_tokens = COALESCE(tok.input_tokens, 0),
       total_output_tokens = COALESCE(tok.output_tokens, 0),
       final_assistant_text = x.content,
       subject_agent = CASE
           WHEN t.subject_agent = 'main' AND cardinality(d.agents) = 1 THEN d.agents[1]
           ELSE t.subject_agent
       END
  FROM tool_totals tt, errors e, dispatched d, findings f, tokens tok, texts x
 WHERE t.id = $1
   AND t.closed_at IS NOT NULL
