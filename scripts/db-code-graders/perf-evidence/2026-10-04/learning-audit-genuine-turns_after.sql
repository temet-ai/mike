SELECT p.project_path AS project,count(*) AS turns,
          count(*) FILTER(WHERE f.prompt_kind='genuine') AS genuine_turns,
          count(*) FILTER(WHERE f.operator_turn_id IS NULL) AS missing_provenance,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND t.live_verdict_id IS NOT NULL AND v.id IS NULL) AS stale_outcome_labels,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='corrected') AS corrected,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='accepted') AS accepted,
          count(*) FILTER(WHERE f.prompt_kind='genuine' AND v.reaction='corrected' AND coalesce(t.final_assistant_text,'')='') AS empty_corrected,
          count(*) FILTER(WHERE t.closed_at IS NOT NULL AND t.embedded_at IS NULL) AS pending_embeddings,
          sum(t.tool_call_count) AS tool_calls,sum(t.tool_error_count) AS tool_errors
          FROM memory.operator_turns t LEFT JOIN projects.projects p ON p.id=t.project_id
          LEFT JOIN memory.turn_facts f ON f.operator_turn_id=t.id
          LEFT JOIN memory.turn_verdicts v ON v.id=t.live_verdict_id AND v.projected
            AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id
          WHERE t.created_at >= '2026-09-27T17:40:00Z' AND t.created_at < '2026-10-04T17:40:00Z' GROUP BY 1 ORDER BY 1;
