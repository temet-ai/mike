SELECT
  count(*) FILTER (WHERE r.request_envelope IS NOT NULL) AS candidate_rows,
  count(DISTINCT r.session_id) FILTER (WHERE r.request_envelope IS NOT NULL) AS sessions_affected,
  COALESCE(sum(octet_length(r.request_body)) FILTER (WHERE r.request_envelope IS NOT NULL), 0) AS reclaimable_bytes,
  count(*) FILTER (WHERE r.request_envelope IS NULL) AS refused_no_envelope_rows,
  COALESCE(sum(octet_length(r.request_body)) FILTER (WHERE r.request_envelope IS NULL), 0) AS refused_no_envelope_bytes
FROM requests.requests r
JOIN sessions.sessions s ON s.id = r.session_id
WHERE r.request_body IS NOT NULL
  AND s.last_seen < now() - make_interval(days => 14)
  AND r.id < (SELECT max(later.id) FROM requests.requests later WHERE later.session_id = r.session_id)
