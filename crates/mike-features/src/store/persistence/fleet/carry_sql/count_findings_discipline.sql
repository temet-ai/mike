WITH hit AS (
    INSERT INTO memory.carried_lesson_findings (lesson_id, note_uuid)
    SELECT c.lesson_id, $1
      FROM memory.carried_lessons c
      JOIN memory.lessons l ON l.id = c.lesson_id AND l.archived_at IS NULL
     WHERE c.project_id = $2
       AND c.tag = ANY($3::text[])
       AND c.source_note_uuid <> $1
       AND c.created_at < $4
       AND c.discipline = $5
    ON CONFLICT (lesson_id, note_uuid) DO NOTHING
    RETURNING lesson_id
)
UPDATE memory.carried_lessons c
   SET later_findings = c.later_findings + 1
  FROM hit
 WHERE c.lesson_id = hit.lesson_id
RETURNING c.lesson_id AS "lesson_id!"
