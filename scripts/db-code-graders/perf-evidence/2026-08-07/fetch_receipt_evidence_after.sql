SELECT request_id,
       created_at,
       convert_from(substring(request_body from greatest(1, pos - 120) for 400), 'UTF8') AS excerpt
  FROM (
      SELECT r.id AS request_id,
             r.created_at,
             r.request_body,
             position(convert_to('62a12706-a194-4e87-967e-1c40f549a287', 'UTF8') in r.request_body) AS pos
        FROM requests.requests r
        JOIN sessions.sessions s ON s.id = r.session_id
       WHERE s.session_uuid = 'ac524070-94ea-4f4a-8bfa-614cdd176679'::uuid
         AND r.created_at >= '2026-08-05T12:12:49Z'::timestamptz
         AND r.request_body IS NOT NULL
         AND position(convert_to('62a12706-a194-4e87-967e-1c40f549a287', 'UTF8') in r.request_body) > 0
       LIMIT 20
  ) hits
 ORDER BY created_at
