SELECT id, request_body
  FROM requests.requests
 WHERE id >= 1
   AND request_body IS NOT NULL
   AND request_envelope IS NULL
 ORDER BY id ASC
 LIMIT 60
