SELECT v.verdict_id, v.operator_turn_id, v.session_id,
        v.project_id, v.confidence, v.rationale, v.unmet_ask,
        v.user_prompt, v.final_assistant_text, v.correcting_message
   FROM memory.lesson_eligible_corrections_v v
  WHERE v.created_at >= '2026-09-04T17:40:00Z'::timestamptz
    AND NOT EXISTS (
        SELECT 1 FROM memory.lesson_proposal_evidence e
        LEFT JOIN memory.lesson_proposal_recovery recovery
          ON recovery.verdict_id = e.verdict_id AND recovery.recovery_state = 'needs_reproposal'
        WHERE e.verdict_id = v.verdict_id AND recovery.verdict_id IS NULL
    )
    AND NOT EXISTS (
        SELECT 1 FROM memory.lesson_proposal_jobs j WHERE j.verdict_id = v.verdict_id
    )
  ORDER BY v.created_at ASC, v.verdict_id ASC
  LIMIT 200