WITH seats AS (
    SELECT rs.seat, rs.session_id AS sid, rs.launched_at, rs.kind
      FROM launch.run_seats rs
     WHERE rs.run_id = $1
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
