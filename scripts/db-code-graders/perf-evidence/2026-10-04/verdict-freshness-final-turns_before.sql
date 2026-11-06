WITH b AS (
    SELECT GREATEST(COALESCE(eb.promoted_at, now()), now() - make_interval(days => 30)) AS since
      FROM eval.bundles eb
     WHERE eb.id = 14
)
SELECT t.id
  FROM memory.operator_turns t
  JOIN b ON t.closed_at >= b.since
  JOIN memory.turn_facts f ON f.operator_turn_id = t.id
  JOIN requests.requests r ON r.id = t.start_request_id
 WHERE t.closed_at IS NOT NULL
   AND t.outcome_source IS NULL
   AND (f.prompt_kind <> 'genuine'
        OR f.session_class = 'synthetic'
        OR r.claude_code_agent_id IS NOT NULL
        OR t.judge_failure_reason = 'permanent')
 ORDER BY t.id
 LIMIT 500;
