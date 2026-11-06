SELECT id, name, session_uuid, folder, backend, model, agent, claude_args,
       created_at, superseded_by, closed_at
  FROM launch.templates
 WHERE superseded_by IS NULL AND closed_at IS NULL
 ORDER BY created_at DESC, id DESC
