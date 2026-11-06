WITH activity AS (
        SELECT claude_code_agent_id AS agent_id,
               max(created_at) AS last_request_at,
               count(*) AS request_count
          FROM requests.requests
         WHERE session_id = 1545259 AND claude_code_agent_id IS NOT NULL
         GROUP BY 1
    ), edits AS (
        SELECT r.claude_code_agent_id AS agent_id, max(u.created_at) AS last_edit_at
          FROM tools.uses u
          JOIN requests.requests r ON r.id = u.request_id
         WHERE u.session_id = 1545259
           AND u.tool_name IN ('Edit', 'Write', 'MultiEdit', 'NotebookEdit')
           AND r.claude_code_agent_id IS NOT NULL
         GROUP BY 1
    )
    SELECT w.kind, w.task_id, w.description, w.owner_agent_id, w.state, w.launched_at,
           w.finished_at, w.exit_code, w.summary, a.last_request_at,
           COALESCE(a.request_count, 0), e.last_edit_at
      FROM agents.work_items w
      LEFT JOIN activity a ON a.agent_id = w.task_id
      LEFT JOIN edits e ON e.agent_id = w.task_id
     WHERE w.session_id = 1545259
     ORDER BY w.launched_at
