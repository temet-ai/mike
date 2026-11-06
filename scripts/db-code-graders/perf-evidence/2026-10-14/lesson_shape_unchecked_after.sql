SELECT l.id, l.text, l.topic
   FROM memory.lessons l
  WHERE l.archived_at IS NULL
    AND NOT EXISTS (SELECT 1 FROM memory.lesson_shape_findings f WHERE f.lesson_id = l.id)
  ORDER BY l.id
  LIMIT 500
