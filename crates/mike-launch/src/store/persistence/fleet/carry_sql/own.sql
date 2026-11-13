SELECT c.lesson_id AS "lesson_id!"
  FROM memory.carried_lessons c
 WHERE c.source_note_uuid = $1
