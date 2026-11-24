WITH renamed AS (
    UPDATE wright.cause_groups g
       SET name = $2::text
     WHERE g.id = $1::bigint
    RETURNING g.id, g.name, g.created_at
)
SELECT r.id, r.name, r.created_at, count(q.cause_id) AS causes
  FROM renamed r
  LEFT JOIN wright.cause_queue q ON q.group_id = r.id
 GROUP BY r.id, r.name, r.created_at
