SELECT r.id, r.created_at
   FROM requests.requests r
   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
  WHERE r.created_at >= '2026-09-17T00:00:00Z' AND r.created_at < '2026-09-24T15:00:00Z'
    AND r.claude_code_agent_id IS NOT NULL
    AND o.code IN ('primary', 'primary_first')
    AND (r.created_at, r.id) > ('2026-09-17T00:00:00Z', 0)
  ORDER BY r.created_at, r.id
  LIMIT 2000;
