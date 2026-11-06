
        WITH base_requests AS (
             SELECT
                 r.id                AS request_id,
                 r.created_at        AS req_created_at,
                 r.subagent_persona,
                 COALESCE(
                     r.claude_code_agent_id,
                     hdr.agent_id_hdr
                 )                   AS agent_id
             FROM requests.requests r
             -- LATERAL: top-1 extraction from a bounded per-request headers array.
             LEFT JOIN LATERAL (
                 SELECT elem->>1 AS agent_id_hdr
                 FROM jsonb_array_elements(r.request_headers) AS elem
                 WHERE elem->>0 = 'x-claude-code-agent-id'
                 LIMIT 1
             ) hdr ON true
             JOIN requests.request_bodies_v rbv ON rbv.request_id = r.id
             WHERE r.session_id = 1173418
               AND rbv.system_text LIKE ANY(ARRAY['%running inside a workflow script%','%spawned by a workflow orchestration script%']::text[]::text[])
         ),
         resolved AS (
             SELECT * FROM base_requests WHERE agent_id IS NOT NULL
         ),
         last_structured_output AS (
             SELECT DISTINCT ON (tu.request_id)
                    tu.request_id,
                    tu.input AS result_json
             FROM tools.uses tu
             WHERE tu.request_id IN (SELECT request_id FROM resolved)
               AND tu.tool_name = 'StructuredOutput'
             ORDER BY tu.request_id, tu.id DESC
         )
         SELECT
             r.agent_id,
             r.request_id,
             r.req_created_at,
             r.subagent_persona,
             lso.result_json
         FROM resolved r
         LEFT JOIN last_structured_output lso ON lso.request_id = r.request_id
         ORDER BY r.req_created_at ASC;
