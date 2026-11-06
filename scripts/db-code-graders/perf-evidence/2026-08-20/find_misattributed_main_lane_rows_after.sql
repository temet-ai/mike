WITH occ AS (
    SELECT r.session_id,
           x.prompt_sha256,
           count(*)  AS occurrences,
           min(r.id) AS first_request_id
      FROM requests.system_prompts x
      JOIN requests.requests r        ON r.id = x.request_id
      JOIN requests.turn_origins o    ON o.id = r.turn_origin_id
      JOIN system_prompts.session_prompts sp
           ON sp.session_id = r.session_id AND sp.lane = 'main'
     WHERE x.prompt_sha256 IS NOT NULL
       AND o.code IN ('primary', 'primary_first')
     GROUP BY r.session_id, x.prompt_sha256
),
winner AS (
    SELECT DISTINCT ON (session_id)
           session_id, prompt_sha256, occurrences, first_request_id
      FROM occ
     ORDER BY session_id, occurrences DESC, first_request_id ASC
)
SELECT sp.id, sp.session_id, sp.request_id,
       w.first_request_id, w.prompt_sha256, w.occurrences
  FROM system_prompts.session_prompts sp
  JOIN winner w ON w.session_id = sp.session_id
 WHERE sp.lane = 'main'
   AND sp.request_id <> w.first_request_id
 ORDER BY sp.session_id
