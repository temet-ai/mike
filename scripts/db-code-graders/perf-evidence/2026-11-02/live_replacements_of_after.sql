WITH RECURSIVE chain AS (
    SELECT 2083::bigint AS id, 0 AS depth
    UNION ALL
    SELECT r.from_lesson_id, c.depth + 1
      FROM chain c
      JOIN memory.lesson_relations r ON r.to_lesson_id = c.id
      JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
      JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
     WHERE c.depth < 12
 )
 SELECT DISTINCT c.id
   FROM chain c
   JOIN memory.lessons l ON l.id = c.id
  WHERE c.id <> 2083 AND l.archived_at IS NULL
  ORDER BY c.id
