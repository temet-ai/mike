SELECT d.request_id AS "request_id!",
       d.ord AS "ord!",
       m.body::text AS "text!"
FROM requests.deltas d
JOIN messages.messages m ON m.id = d.message_id
WHERE d.session_id = $1
  AND d.request_id = ANY($2)
  AND d.change_kind_id = 1
  AND (CASE WHEN $4 THEN strpos(m.body::text, $3)
            ELSE strpos(lower(m.body::text), lower($3)) END) > 0
  AND NOT EXISTS (
        SELECT 1
        FROM requests.deltas e
        WHERE e.session_id = d.session_id
          AND e.message_id = d.message_id
          AND e.change_kind_id = 1
          AND (e.request_id, e.ord) < (d.request_id, d.ord)
      )
ORDER BY d.request_id, d.ord
