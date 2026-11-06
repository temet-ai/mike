SELECT u.file_path, s.session_uuid::text AS session_uuid, u.request_id, u.tool_name, u.created_at,
       a.name AS agent_name, r.subagent_persona,
       (CASE WHEN u.input->>'new_string' IS NOT NULL AND u.input->>'old_string' IS NOT NULL
        THEN COALESCE(array_length(string_to_array(u.input->>'new_string', chr(10)),1),0)
           - COALESCE(array_length(string_to_array(u.input->>'old_string', chr(10)),1),0)
        END)::int AS line_delta
  FROM tools.uses u
  JOIN requests.requests r ON r.id = u.request_id
  JOIN sessions.sessions s ON s.id = r.session_id
  LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
  LEFT JOIN agents.agents a ON a.id = ai.agent_id
 WHERE u.operation='write' AND u.file_path = ANY(ARRAY['/Users/vimes/projects/temet/temet-proxy/crates/mike/src/api/handlers.rs','/Users/vimes/projects/temet/temet-proxy/frontend/src/lib/components/graph/EmbeddingGraph.svelte'])
 ORDER BY u.file_path, u.created_at, u.id;
