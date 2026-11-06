SELECT tu.tool_name,
       NULLIF(tu.input->>'skill', '') AS skill,
       count(*) AS use_count,
       count(*) FILTER (WHERE tr.is_error) AS error_count,
       min(tu.request_id) AS first_request_id,
       max(tu.request_id) AS last_request_id,
       min(tu.created_at) AS first_used_at,
       max(tu.created_at) AS last_used_at
  FROM tools.uses tu
  LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
 WHERE tu.session_id = 1776684
   AND (tu.tool_name = 'Skill' OR tu.tool_name = ANY(ARRAY['mcp__mike__recall_project_context','mcp__mike__recall_similar_turns','mcp__mike__search_project_knowledge','mcp__mike__read_project_knowledge']))
 GROUP BY tu.tool_name, NULLIF(tu.input->>'skill', '')
 ORDER BY min(tu.created_at), tu.tool_name, 2
