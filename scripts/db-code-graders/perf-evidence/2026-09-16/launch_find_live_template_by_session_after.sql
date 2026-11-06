SELECT id, name, session_uuid, folder, backend, model, agent, claude_args,
       created_at, superseded_by, closed_at
  FROM launch.templates
 WHERE session_uuid = 'd34dbeef-0000-4000-8000-000000000000'::uuid AND superseded_by IS NULL AND closed_at IS NULL
