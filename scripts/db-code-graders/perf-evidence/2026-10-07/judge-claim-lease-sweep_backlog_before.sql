SELECT t.id FROM memory.operator_turns t
   JOIN memory.turn_facts f ON f.operator_turn_id = t.id
   JOIN requests.requests r ON r.id = t.start_request_id
  WHERE t.closed_at IS NOT NULL
    AND f.prompt_kind = 'genuine'
    AND f.session_class <> 'synthetic'
    AND r.claude_code_agent_id IS NULL
    AND f.next_state IN ('resolved', 'none_session_settled') AND t.judge_failure_reason IS DISTINCT FROM 'permanent' AND NOT EXISTS (
        SELECT 1 FROM memory.turn_verdicts v
         WHERE v.operator_turn_id = t.id AND v.mode = 'live'
           AND v.bundle_id = 9999 AND v.reaction IS NOT NULL
           AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id) AND (t.judge_next_attempt_at IS NULL OR t.judge_next_attempt_at <= now()) AND t.closed_at >= '2026-09-30T09:36:00Z'::timestamptz ORDER BY t.closed_at ASC, t.id ASC LIMIT 16;
