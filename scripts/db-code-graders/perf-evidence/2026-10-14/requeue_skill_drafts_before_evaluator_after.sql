WITH reset AS (
    UPDATE memory.skill_drafts d
       SET status = 'draft', eval = NULL, evaluated_at = NULL,
           eval_progress = NULL
     WHERE d.status = 'rejected'
       AND d.evaluated_at IS NOT NULL
       AND COALESCE((d.eval ->> 'evaluator_version')::integer, 0) < 2
 RETURNING d.id),
cleared AS (
    DELETE FROM memory.skill_draft_eval_failures f
     USING reset
     WHERE f.draft_id = reset.id
 RETURNING f.draft_id)
SELECT count(*) AS requeued FROM reset
