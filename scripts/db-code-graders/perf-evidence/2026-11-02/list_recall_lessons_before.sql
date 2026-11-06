(SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text,
        l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic
   FROM memory.lessons l
   LEFT JOIN projects.projects p ON p.id = l.project_id
   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  WHERE l.project_id = 993478
    AND l.topic = 'ui-design'
    AND l.archived_at IS NULL
    AND (l.expires_at IS NULL OR l.expires_at > NOW())
  ORDER BY l.created_at DESC)
UNION ALL
(SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text,
        l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic
   FROM memory.lessons l
   LEFT JOIN projects.projects p ON p.id = l.project_id
   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
  WHERE l.project_id = 993478
    AND l.topic = 'db-ports'
    AND l.archived_at IS NULL
    AND (l.expires_at IS NULL OR l.expires_at > NOW())
  ORDER BY l.created_at DESC)
