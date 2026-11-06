WITH k AS (
        SELECT * FROM unnest(ARRAY[1545259]::bigint[], ARRAY[now() - interval '11 minutes']::timestamptz[]) AS k(session_id, after)
    )
    SELECT r.session_id, r.claude_code_agent_id,
           max(r.created_at) AS last_request_at,
           max(r.created_at) FILTER (
               WHERE r.claude_code_agent_id IS NULL
                 AND torg.code IN ('primary', 'primary_first')
           ) AS main_last_request_at,
           max(u.created_at) AS last_edit_at
      FROM k
      JOIN requests.requests r
        ON r.session_id = k.session_id AND r.created_at > k.after
      LEFT JOIN requests.turn_origins torg ON torg.id = r.turn_origin_id
      LEFT JOIN tools.uses u
        ON u.request_id = r.id
       AND r.claude_code_agent_id IS NOT NULL
       AND u.tool_name IN ('Edit', 'Write', 'MultiEdit', 'NotebookEdit')
     GROUP BY r.session_id, r.claude_code_agent_id
