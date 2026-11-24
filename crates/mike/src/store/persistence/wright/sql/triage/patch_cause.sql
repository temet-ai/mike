UPDATE wright.causes c
   SET pinned_at = CASE
           WHEN $2::boolean IS NULL THEN c.pinned_at
           WHEN $2::boolean THEN COALESCE(c.pinned_at, now())
           ELSE NULL
       END,
       operator_priority = CASE WHEN $3::boolean THEN $4::float8 ELSE c.operator_priority END,
       group_id = CASE WHEN $5::boolean THEN $6::bigint ELSE c.group_id END
 WHERE c.id = $1::bigint
   AND c.merged_into IS NULL
   AND EXISTS (
       SELECT 1
         FROM wright.problems p
         JOIN wright.episodes e ON e.problem_id = p.id AND e.closed_at IS NULL
        WHERE p.cause_id = c.id
          AND p.state = 'open')
RETURNING c.id
