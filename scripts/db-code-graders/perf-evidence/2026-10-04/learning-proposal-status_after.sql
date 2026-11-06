UPDATE memory.lesson_proposal_jobs
         SET status = CASE WHEN NULL::bigint::bigint IS NULL THEN 'rejected' ELSE 'applied' END,
             updated_at = NOW() WHERE verdict_id = 4