SELECT c.lesson_id AS "lesson_id"
  FROM memory.carried_lessons c
 WHERE c.source_note_uuid = 'b25d8d93-b03a-41e7-9205-c67fbc8cc241'::uuid
