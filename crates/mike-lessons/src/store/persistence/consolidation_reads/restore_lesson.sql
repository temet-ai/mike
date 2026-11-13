UPDATE memory.lessons l
   SET stage_id           = ls.id,
       archived_at        = NULL,
       archived_reason_id = NULL,
       duplicate_of_id    = NULL
  FROM memory.lesson_stages ls
 WHERE ls.code = 'long_term'
   AND l.id = $1
   AND l.archived_at IS NOT NULL
RETURNING l.id AS "id!"
