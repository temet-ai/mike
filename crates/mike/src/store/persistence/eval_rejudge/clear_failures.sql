WITH e AS (
    SELECT t.id,
           EXISTS (SELECT 1 FROM memory.turn_verdicts v
                    WHERE v.operator_turn_id = t.id AND v.mode = 'live' AND v.bundle_id = $3
                      AND v.reaction IS NOT NULL
                      AND v.next_genuine_request_id IS NOT DISTINCT FROM f.next_genuine_request_id)
               AS judged
      FROM memory.operator_turns t
      JOIN memory.turn_facts f ON f.operator_turn_id = t.id
      JOIN requests.requests r ON r.id = t.start_request_id
     WHERE t.closed_at IS NOT NULL
       AND f.prompt_kind = 'genuine'
       AND f.session_class <> 'synthetic'
       AND r.claude_code_agent_id IS NULL
       AND f.next_state IN ('resolved', 'none_session_settled')
       AND t.closed_at >= $1 AND t.closed_at < $2
)
UPDATE memory.operator_turns u
   SET judge_failure_reason = NULL, judge_attempt_count = 0, judge_next_attempt_at = NULL,
       judge_queued_at = CASE WHEN e.judged THEN u.judge_queued_at
                              ELSE COALESCE(u.judge_queued_at, now()) END
  FROM e
 WHERE u.id = e.id
   AND (u.judge_failure_reason IS NOT NULL OR u.judge_next_attempt_at IS NOT NULL
        OR (u.judge_queued_at IS NULL AND NOT e.judged))
