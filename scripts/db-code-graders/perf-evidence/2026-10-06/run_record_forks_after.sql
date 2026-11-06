WITH forks AS (
    SELECT l.id, l.template_id, l.created_at, s.id AS sid
      FROM launch.launches l JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE l.run_id = 'fleet-change-record-2-f9009fa8' AND l.template_id IS NOT NULL
),
first_request AS (
    SELECT DISTINCT ON (f.template_id) f.template_id, q.created_at AS at
      FROM forks f JOIN requests.requests q ON q.session_id = f.sid AND q.created_at >= f.created_at
     ORDER BY f.template_id, q.created_at
)
SELECT count(*) FILTER (WHERE f.created_at >= r.at) AS "waited", count(*) AS "forks"
  FROM forks f LEFT JOIN first_request r ON r.template_id = f.template_id
