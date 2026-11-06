SELECT p.project_path AS project,count(*) AS turns,
          count(*) FILTER(WHERE t.reaction='corrected') AS corrected,
          count(*) FILTER(WHERE t.reaction='accepted') AS accepted,
          count(*) FILTER(WHERE t.reaction='corrected' AND coalesce(t.final_assistant_text,'')='') AS empty_corrected,
          count(*) FILTER(WHERE t.closed_at IS NOT NULL AND t.embedded_at IS NULL) AS pending_embeddings,
          sum(t.tool_call_count) AS tool_calls,sum(t.tool_error_count) AS tool_errors
          FROM memory.operator_turns t LEFT JOIN projects.projects p ON p.id=t.project_id
          WHERE t.created_at >= '2026-09-27T17:40:00Z' AND t.created_at < '2026-10-04T17:40:00Z' GROUP BY 1 ORDER BY 1