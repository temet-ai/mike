WITH activity AS MATERIALIZED (
    SELECT MAX(r.created_at) AS last_request_at,
           MAX(r.created_at) FILTER (
               WHERE ro.code IN ('primary', 'primary_first')
           ) AS last_eligible_turn_at
      FROM requests.requests r
      JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
      JOIN sessions.sessions s ON s.id = r.session_id
     WHERE s.session_uuid = '2875c812-fc8a-47d6-b3da-6044f5e836d7'
)
SELECT a.last_request_at       AS last_request_at,
       a.last_eligible_turn_at AS last_eligible_turn_at
  FROM sessions.sessions s
  CROSS JOIN activity a
 WHERE s.session_uuid = '2875c812-fc8a-47d6-b3da-6044f5e836d7';
