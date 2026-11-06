SELECT l.id, l.project_id, l.text, k.code AS kind, l.topic, a.name AS agent_name
  FROM memory.lessons l
  JOIN memory.lesson_kinds k ON k.id = l.kind_id
  LEFT JOIN agents.agents a ON a.id = l.agent_id
  LEFT JOIN memory.lesson_placement_failures f ON f.lesson_id = l.id
 WHERE l.placement_id IS NULL
   AND l.archived_at IS NULL
   AND l.project_id IS NOT NULL
   AND COALESCE(l.topic, '') NOT LIKE 'verifier:%'
   AND (f.lesson_id IS NULL
        OR (f.attempts < 3
            AND f.last_failed_at <= now() - interval '1 hour' * (2 ^ f.attempts)))
 ORDER BY l.id
 LIMIT 5
