SELECT c.lesson_id AS "lesson_id!"
  FROM memory.carried_lessons c
  JOIN memory.lessons l ON l.id = c.lesson_id AND l.archived_at IS NULL
 WHERE c.project_id = $1
   AND c.tag = $2
   AND c.discipline = $3
