SELECT DISTINCT r.session_id
   FROM requests.requests r
   JOIN requests.turn_types tt ON tt.id = r.turn_type_id AND tt.code = 'user_prompt'
   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
                               AND o.code IN ('primary', 'primary_first')
  WHERE r.created_at >= '2026-07-27 00:00:00+00'
    AND r.created_at < '2026-09-24 16:05:00+00'
  ORDER BY r.session_id
