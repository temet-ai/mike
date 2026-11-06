SELECT r.id, r.request_body
  FROM requests.requests r
  JOIN requests.turn_origins o ON o.id = r.turn_origin_id
 WHERE r.id > 0
   AND o.code IN ('primary', 'primary_first')
   AND r.message_count <= 2
   AND r.request_body IS NOT NULL
 ORDER BY r.id ASC
 LIMIT 300
