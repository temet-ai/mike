SELECT v.id AS verdict_id, v.operator_turn_id, v.session_id,
        s.project_id, v.confidence, v.rationale, v.unmet_ask,
        ot.user_prompt, ot.final_assistant_text, r.human_prompt AS correcting_message
   FROM memory.turn_verdicts v
   JOIN sessions.sessions s ON s.id = v.session_id
   JOIN memory.operator_turns ot ON ot.id = v.operator_turn_id
   JOIN memory.turn_facts tf ON tf.operator_turn_id = v.operator_turn_id
   LEFT JOIN requests.requests r ON r.id = tf.next_genuine_request_id
  WHERE v.mode = 'live'
    AND v.reaction = 'corrected'
    AND v.created_at >= '2026-08-29 00:00:00+00'
    AND tf.next_state = 'resolved'
    AND NOT EXISTS (
        SELECT 1 FROM memory.lesson_proposal_evidence e WHERE e.verdict_id = v.id
    )
  ORDER BY v.created_at ASC, v.id ASC
  LIMIT 200;
