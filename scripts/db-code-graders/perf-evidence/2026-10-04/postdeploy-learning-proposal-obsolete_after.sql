UPDATE memory.lesson_proposal_recovery r SET recovery_state = 'obsolete',
                 disposition_reason = 'No longer the current eligible human correction'
             WHERE r.recovery_state = 'needs_reproposal' AND NOT EXISTS (
                 SELECT 1 FROM memory.lesson_eligible_corrections_v v WHERE v.verdict_id = r.verdict_id
             )