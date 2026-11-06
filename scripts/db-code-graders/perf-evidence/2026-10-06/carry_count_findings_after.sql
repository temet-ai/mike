WITH hit AS (
    INSERT INTO memory.carried_lesson_findings (lesson_id, note_uuid)
    SELECT c.lesson_id, '9e8f740e-d809-4bec-9cc2-f9a4148dcc82'::uuid
      FROM memory.carried_lessons c
      JOIN memory.lessons l ON l.id = c.lesson_id AND l.archived_at IS NULL
     WHERE c.project_id = 993478
       AND c.tag = ANY(ARRAY['carry:item-574']::text[]::text[])
       AND c.source_note_uuid <> '9e8f740e-d809-4bec-9cc2-f9a4148dcc82'::uuid
       AND c.created_at < now()
       AND ('db-dev'::text IS NULL OR c.discipline = 'db-dev')
    ON CONFLICT (lesson_id, note_uuid) DO NOTHING
    RETURNING lesson_id
)
UPDATE memory.carried_lessons c
   SET later_findings = c.later_findings + 1
  FROM hit
 WHERE c.lesson_id = hit.lesson_id
RETURNING c.lesson_id AS "lesson_id"
