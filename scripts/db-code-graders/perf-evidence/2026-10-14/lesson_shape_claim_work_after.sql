SELECT f.lesson_id, l.text, l.topic, f.shape, f.detail, f.attempts,
        lk.code AS kind, ls.code AS stage, src.code AS source,
        l.project_id, l.session_id, s.session_uuid
   FROM memory.lesson_shape_findings f
   JOIN memory.lessons l ON l.id = f.lesson_id AND l.archived_at IS NULL
   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
   JOIN memory.lesson_sources src ON src.id = l.source_id
   LEFT JOIN sessions.sessions s ON s.id = l.session_id
  WHERE f.resolution IS NULL
    AND f.attempts < 3
    AND (f.last_attempt_at IS NULL
         OR f.last_attempt_at <= now() - interval '1 hour' * (2 ^ f.attempts))
  ORDER BY (ls.code = 'raw') DESC, f.found_at, f.lesson_id
  LIMIT 3
