WITH seats AS (
    SELECT DISTINCT ON (s.id) l.seat, s.id AS sid, l.created_at AS launched_at,
           CASE WHEN l.seat LIKE 'coordinator%' THEN 'coordinator'
                WHEN l.seat LIKE 'observer%' THEN 'observer'
                WHEN l.seat LIKE 'acceptance%' THEN 'acceptance'
                WHEN l.seat LIKE 'template-%' THEN 'template'
                WHEN l.seat = 'reference' THEN 'reference'
                ELSE 'worker' END AS kind
      FROM launch.launches l
      JOIN sessions.sessions s ON s.session_uuid = l.session_uuid
     WHERE l.run_id = $1 AND l.seat IS NOT NULL AND l.seat <> 'status'
     ORDER BY s.id, l.created_at, l.id
)
SELECT s.kind AS "kind!",
       count(*) AS "requests!",
       coalesce(sum(p.input_tokens), 0)::bigint AS "input_tokens!",
       coalesce(sum(p.output_tokens), 0)::bigint AS "output_tokens!",
       coalesce(sum(p.cache_read_input_tokens), 0)::bigint AS "cache_read_tokens!",
       coalesce(sum(p.cache_creation_input_tokens), 0)::bigint AS "cache_write_tokens!"
  FROM seats s
  JOIN requests.requests q ON q.session_id = s.sid AND q.created_at >= s.launched_at
  JOIN responses.responses p ON p.request_id = q.id
 GROUP BY s.kind
 ORDER BY s.kind
