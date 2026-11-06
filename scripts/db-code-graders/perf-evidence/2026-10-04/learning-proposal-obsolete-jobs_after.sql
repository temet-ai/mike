UPDATE memory.lesson_proposal_jobs j SET status = 'rejected', updated_at = NOW()
             WHERE j.status = 'awaiting_embedding' AND NOT EXISTS (
                 SELECT 1 FROM memory.lesson_eligible_corrections_v v WHERE v.verdict_id = j.verdict_id
             )