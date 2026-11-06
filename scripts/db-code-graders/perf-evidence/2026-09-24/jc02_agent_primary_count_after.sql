SELECT count(*)
   FROM requests.requests r
   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                               AND o.code IN ('primary', 'primary_first')
  WHERE r.created_at >= '2026-09-17 00:00:00+00'
    AND r.claude_code_agent_id IS NOT NULL
