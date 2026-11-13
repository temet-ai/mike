SELECT s.session_uuid::text      AS "session_id!",
       r.id,
       m.code                    AS method,
       r.path,
       r.model,
       r.is_streaming,
       r.message_count,
       st.code                   AS status,
       r.params,
       r.human_prompt,
       tt.code                   AS turn_type,
       torg.code                 AS turn_origin,
       r.subagent_persona,
       r.created_at,
       r.completed_at,
       rsp.prompt_sha256         AS "prompt_sha256?",
       pc.id                     AS "composition_id?",
       r.predecessor_request_id,
       r.request_number,
       r.agent_instance_id,
       tu_dispatch.request_id    AS "dispatch_request_id?",
       COALESCE(p.project_path, '(no project)') AS "project_path!"
  FROM requests.requests r
  JOIN requests.methods m ON m.id = r.method_id
  JOIN requests.statuses st ON st.id = r.status_id
  JOIN requests.turn_types tt ON tt.id = r.turn_type_id
  JOIN requests.turn_origins torg ON torg.id = r.turn_origin_id
  JOIN sessions.sessions s ON s.id = r.session_id
  LEFT JOIN requests.system_prompts rsp ON rsp.request_id = r.id
  LEFT JOIN overrides.overrides pc ON pc.id = rsp.override_id
  LEFT JOIN projects.projects p ON p.id = s.project_id
  LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
  LEFT JOIN agents.dispatches ad ON ad.id = ai.dispatch_id
  LEFT JOIN tools.uses tu_dispatch ON tu_dispatch.id = ad.tool_use_id
 WHERE r.session_id = $1
 ORDER BY r.created_at DESC, r.id DESC
 LIMIT $2 OFFSET $3
