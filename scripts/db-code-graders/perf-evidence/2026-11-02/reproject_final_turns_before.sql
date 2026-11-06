SELECT t.id
  FROM memory.operator_turns t
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  JOIN requests.requests r ON r.id = t.start_request_id
  LEFT JOIN LATERAL (
      SELECT v.bundle_id, v.next_genuine_request_id
        FROM memory.turn_verdicts v WHERE v.id = t.live_verdict_id OFFSET 0
  ) v ON true
 WHERE t.closed_at >= now() - make_interval(days => 7)
   AND (
       (t.outcome_source IS NULL AND
           (f.prompt_kind <> 'genuine' OR f.session_class = 'synthetic'
            OR r.claude_code_agent_id IS NOT NULL OR t.judge_failure_reason = 'permanent'))
       OR (t.outcome_source IN ('judge', 'frustration_close') AND
           (f.prompt_kind <> 'genuine' OR f.session_class = 'synthetic'
            OR r.claude_code_agent_id IS NOT NULL
            OR f.next_state NOT IN ('resolved', 'none_session_settled')
            OR (t.outcome_source = 'frustration_close' AND f.next_state <> 'none_session_settled')
            OR (v.bundle_id = 94 AND v.next_genuine_request_id
                IS DISTINCT FROM f.next_genuine_request_id)))
   )
 ORDER BY t.id
 LIMIT 500;
