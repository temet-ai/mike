WITH f AS (
    INSERT INTO memory.skill_draft_eval_failures (draft_id, last_error)
    VALUES (4, 'model: 502')
    ON CONFLICT (draft_id) DO UPDATE
       SET attempts = memory.skill_draft_eval_failures.attempts + 1,
           last_error = EXCLUDED.last_error,
           last_failed_at = now()
    RETURNING attempts)
UPDATE memory.skill_drafts d
   SET status = CASE WHEN f.attempts >= 3 THEN 'rejected' ELSE d.status END,
       eval = CASE WHEN f.attempts >= 3
                   THEN jsonb_build_object('error', 'model: 502'::text, 'attempts', f.attempts)
                   ELSE d.eval END,
       evaluated_at = CASE WHEN f.attempts >= 3 THEN now() ELSE d.evaluated_at END,
       eval_progress = CASE WHEN f.attempts >= 3 THEN NULL ELSE d.eval_progress END
  FROM f
 WHERE d.id = 4 AND d.status = 'evaluating'
RETURNING f.attempts AS attempts, d.status AS status
