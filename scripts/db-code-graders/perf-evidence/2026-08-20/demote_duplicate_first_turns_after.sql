WITH ranked AS (
    SELECT r.id,
           row_number() OVER (
               PARTITION BY r.session_id ORDER BY r.id
           ) AS rn
      FROM requests.requests r
      JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE o.code = 'primary_first'
)
UPDATE requests.requests r
   SET turn_origin_id = p.id
  FROM ranked, requests.turn_origins p
 WHERE ranked.rn > 1
   AND r.id = ranked.id
   AND p.code = 'primary'
