UPDATE memory.skill_drafts SET eval_progress = '{"trigger":[],"prompts":[],"models":["m"]}'::jsonb WHERE id = 1 AND status = 'evaluating'
