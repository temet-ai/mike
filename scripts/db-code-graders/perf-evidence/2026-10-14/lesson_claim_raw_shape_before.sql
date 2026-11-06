SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text,
     l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic
                   FROM memory.lessons l
     LEFT JOIN projects.projects p ON p.id = l.project_id
     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE l.project_id = 993478
                    AND ls.code = 'raw'
                    AND l.embedded_at IS NULL
                    AND l.source_id NOT IN (SELECT id FROM memory.lesson_sources WHERE code = 'carry')
                  ORDER BY l.created_at ASC
                  LIMIT 50
                  FOR UPDATE OF l SKIP LOCKED
