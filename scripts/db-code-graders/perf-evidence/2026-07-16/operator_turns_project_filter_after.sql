
    SELECT
        t.id AS turn_id, s.session_uuid::text AS session_id, t.project_id,
        t.start_request_id, t.end_request_id,
        COALESCE(num.start_request_number, 0) AS start_request_number,
        COALESCE(num.end_request_number,   0) AS end_request_number,
        COALESCE(tx.user_prompt, '')          AS user_prompt,
        COALESCE(tx.final_assistant_text, '') AS final_assistant_text,
        COALESCE(agg.tool_call_count, 0)::int      AS tool_call_count,
        COALESCE(agg.tool_error_count, 0)::int     AS tool_error_count,
        COALESCE(agg.files_touched,     '{}'::text[]) AS files_touched,
        COALESCE(agg.tools_used,        '{}'::text[]) AS tools_used,
        COALESCE(agg.agents_dispatched, '{}'::text[]) AS agents_dispatched,
        COALESCE(agg.skills_used,       '{}'::text[]) AS skills_used,
        COALESCE(agg.weasel_findings_count, 0)::int     AS weasel_findings_count,
        COALESCE(agg.total_input_tokens, 0)::bigint     AS total_input_tokens,
        COALESCE(agg.total_output_tokens, 0)::bigint    AS total_output_tokens,
        COALESCE(EXTRACT(EPOCH FROM (er.completed_at - sr.created_at))::bigint * 1000, 0) AS duration_ms,
        stp_text.stop_response,
        stp.stop_reason,
        stp.stop_request_id,
        (toc.code = 'abandoned' OR COALESCE(stp.unsettled, false)) AS interrupted,
        toc.code AS outcome,
        ev.last_user_message AS outcome_evidence,
        t.classified_at,
        t.created_at,
        COALESCE(t.classified_at, t.created_at) AS updated_at,
        pk_kinds.code AS prompt_kind,
        t.subject_command,
        t.subject_agent,
        sr.model AS model
      FROM memory.operator_turns t
      JOIN sessions.sessions s ON s.id = t.session_id
      JOIN memory.turn_outcomes toc ON toc.id = t.outcome_id
      LEFT JOIN claude_proxy.operator_turn_aggregates_v agg ON agg.turn_id = t.id
      LEFT JOIN claude_proxy.operator_turn_text_v       tx  ON tx.turn_id  = t.id
      LEFT JOIN requests.requests                   ev
             ON ev.id = t.outcome_evidence_request_id
      LEFT JOIN requests.requests                   sr
             ON sr.id = t.start_request_id
      LEFT JOIN requests.requests                   er
             ON er.id = t.end_request_id AND er.completed_at IS NOT NULL
      LEFT JOIN prompt_kinds.classifications pk_cls
             ON pk_cls.request_id = t.start_request_id
      LEFT JOIN prompt_kinds.kinds pk_kinds
             ON pk_kinds.id = pk_cls.kind_id
      LEFT JOIN LATERAL (
            SELECT COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (sr.created_at, sr.id)) AS start_request_number,
                   COUNT(*) FILTER (WHERE (r_num.created_at, r_num.id) <= (er.created_at, er.id)) AS end_request_number
            FROM requests.requests r_num
            WHERE r_num.session_id = t.session_id
      ) num ON true
      LEFT JOIN LATERAL (
            SELECT MIN(t2.start_request_id) AS next_start
              FROM memory.operator_turns t2
             WHERE t2.session_id = t.session_id
               AND t2.start_request_id > t.start_request_id
      ) nxt ON true
      LEFT JOIN LATERAL (
            SELECT r.id AS stop_request_id,
                   (resp.id IS NULL) AS unsettled,
                   CASE WHEN resp.id IS NULL THEN 'aborted' ELSE rsr.code END AS stop_reason
              FROM requests.requests r
              JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
              LEFT JOIN responses.responses    resp ON resp.request_id = r.id
              LEFT JOIN responses.stop_reasons rsr  ON rsr.id = resp.stop_reason_id
             WHERE r.session_id = t.session_id
               AND r.id >= t.start_request_id
               AND (nxt.next_start IS NULL OR r.id < nxt.next_start)
               AND ro.code IN ('primary', 'primary_first')
               AND EXISTS (
                   SELECT 1 FROM responses.stream_blocks b2
                   JOIN responses.stream_block_types bt2 ON bt2.id = b2.block_type_id
                   WHERE b2.request_id = r.id AND bt2.code = 'text')
             ORDER BY r.id DESC
             LIMIT 1
      ) stp ON true
      LEFT JOIN LATERAL (
            SELECT string_agg(b.content, '' ORDER BY b.seq_ord) AS stop_response
              FROM responses.stream_blocks b
              JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
             WHERE b.request_id = stp.stop_request_id AND bt.code = 'text'
      ) stp_text ON stp.stop_request_id IS NOT NULL
                 JOIN projects.projects p ON p.id = t.project_id
                 WHERE 1 = 1
                 AND p.project_path ILIKE '%' || 'am-night-watch/tools/pipeline-utilities' || '%'
                 AND t.created_at >= '2026-07-09T00:00:00Z'::timestamptz
                 AND t.created_at < '2026-07-17T00:00:00Z'::timestamptz
                 AND toc.code = 'corrected'
                 ORDER BY t.created_at DESC LIMIT 50 OFFSET 0
