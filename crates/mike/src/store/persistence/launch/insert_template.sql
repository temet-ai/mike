INSERT INTO launch.templates (name, session_uuid, folder, backend, model, agent, claude_args)
VALUES ($1, $2, $3, $4, $5, $6, $7)
RETURNING id
