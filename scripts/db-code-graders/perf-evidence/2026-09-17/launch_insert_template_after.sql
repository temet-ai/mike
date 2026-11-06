INSERT INTO launch.templates (name, session_uuid, folder, backend, model, agent, claude_args)
VALUES ('verifier', gen_random_uuid(), '/repo/proj', 'herdr', 'opus', NULL, '["--model","opus"]'::jsonb)
RETURNING id
