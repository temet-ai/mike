WITH caller AS (
    SELECT s.id AS session_id
      FROM sessions.sessions s
     WHERE s.session_uuid = $1::uuid
), call_request AS (
    SELECT u.request_id
      FROM tools.uses u
      JOIN caller c ON c.session_id = u.session_id
     WHERE u.created_at > now() - interval '2 minutes'
       AND u.tool_name ~ ('(^|__)' || $2::text || '$')
       AND COALESCE(u.input ->> 'query', '') = $3::text
     ORDER BY u.id DESC
     LIMIT 1
), newest_request AS (
    SELECT r.id AS request_id
      FROM requests.requests r
      JOIN caller c ON c.session_id = r.session_id
     ORDER BY r.id DESC
     LIMIT 1
)
INSERT INTO memory.search_events (session_id, request_id, tool, query, kind_filter, returned, rerank_state)
SELECT (SELECT session_id FROM caller),
       COALESCE((SELECT request_id FROM call_request), (SELECT request_id FROM newest_request)),
       $2::text, $3::text, $4::text, $5::jsonb, $6::text
RETURNING id AS "id!"
