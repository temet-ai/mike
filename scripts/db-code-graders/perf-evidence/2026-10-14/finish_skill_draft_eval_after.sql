UPDATE memory.skill_drafts SET status = 'accepted', eval = '{"verdict":{"accepted":true}}'::jsonb, evaluated_at = now(), eval_progress = NULL WHERE id = 1 AND status = 'evaluating'
