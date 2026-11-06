SELECT id, name, session_uuid, folder, backend, model, agent, claude_args,
       created_at, superseded_by, closed_at
  FROM launch.templates
 WHERE name = 'verifier' AND superseded_by IS NULL AND closed_at IS NULL
