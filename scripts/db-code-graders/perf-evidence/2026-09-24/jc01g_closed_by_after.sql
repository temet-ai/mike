SELECT t.id AS turn_id, t.start_request_id,
        t.outcome_evidence_request_id AS closing_request_id,
        tot.code AS outcome, t.judge_model, t.judge_rubric_version
   FROM requests.requests closer
   JOIN memory.operator_turns t ON t.outcome_evidence_request_id = closer.id
   JOIN requests.requests s ON s.id = t.start_request_id
   LEFT JOIN memory.turn_outcomes tot ON tot.id = t.outcome_id
  WHERE closer.created_at >= '2026-09-17T00:00:00Z' AND closer.created_at < '2026-09-24T15:00:00Z'
    AND closer.claude_code_agent_id IS NOT NULL
    AND s.claude_code_agent_id IS NULL
  ORDER BY t.id;
