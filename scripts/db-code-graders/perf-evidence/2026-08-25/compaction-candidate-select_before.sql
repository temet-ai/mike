SELECT r.id, octet_length(r.request_body) AS bytes
  FROM requests.requests r
  JOIN sessions.sessions s ON s.id = r.session_id
 WHERE r.id >= 1
   AND r.request_body IS NOT NULL
   AND r.request_envelope IS NOT NULL
   AND s.last_seen < now() - make_interval(days => 14)
   AND r.id < (SELECT max(later.id) FROM requests.requests later WHERE later.session_id = r.session_id)
 ORDER BY r.id ASC
 LIMIT 500
