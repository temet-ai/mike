UPDATE memory.operator_turns t
        SET tool_call_count = (
                SELECT COUNT(*) FROM tools.uses u
                JOIN requests.requests r ON r.id = u.request_id
                WHERE r.operator_turn_id = t.id),
            tool_error_count = (
                SELECT COUNT(DISTINCT u.id)
                FROM tools.uses u
                JOIN requests.requests r ON r.id = u.request_id
                JOIN tools.results res ON res.tool_use_id = u.id
                WHERE r.operator_turn_id = t.id AND res.is_error),
            weasel_findings_count = (
                SELECT COUNT(*) FROM requests.diagnostics d
                JOIN requests.requests r ON r.id = d.request_id
                WHERE r.operator_turn_id = t.id AND d.analyzer = 'weasel'),
            tools_used = COALESCE((
                SELECT ARRAY_AGG(DISTINCT u.tool_name ORDER BY u.tool_name)
                FROM tools.uses u
                JOIN requests.requests r ON r.id = u.request_id
                WHERE r.operator_turn_id = t.id AND u.tool_name IS NOT NULL), '{}'),
            agents_dispatched = COALESCE((
                SELECT ARRAY_AGG(DISTINCT ag.name ORDER BY ag.name)
                FROM agents.dispatches ad
                JOIN tools.uses u ON u.id = ad.tool_use_id
                JOIN requests.requests r ON r.id = u.request_id
                JOIN agents.agents ag ON ag.id = ad.agent_id
                WHERE r.operator_turn_id = t.id), '{}'),
            files_touched = COALESCE((
                SELECT ARRAY_AGG(DISTINCT u.file_path ORDER BY u.file_path)
                FROM tools.uses u
                JOIN requests.requests r ON r.id = u.request_id
                WHERE r.operator_turn_id = t.id AND u.file_path IS NOT NULL), '{}'),
            total_input_tokens = COALESCE((
                SELECT SUM(resp.input_tokens)
                FROM responses.responses resp
                JOIN requests.requests r ON r.id = resp.request_id
                WHERE r.operator_turn_id = t.id), 0),
            total_output_tokens = COALESCE((
                SELECT SUM(resp.output_tokens)
                FROM responses.responses resp
                JOIN requests.requests r ON r.id = resp.request_id
                WHERE r.operator_turn_id = t.id), 0),
            final_assistant_text = (
                SELECT string_agg(b.content, '' ORDER BY b.request_id, b.seq_ord)
                FROM responses.stream_blocks b
                JOIN requests.requests r ON r.id = b.request_id
                JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
                WHERE r.operator_turn_id = t.id AND bt.code = 'text'
                  AND o.code NOT LIKE '%sidecar')
        WHERE t.id = 48169
;
