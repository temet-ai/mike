SELECT l.id, l.project_id, p.project_path, lk.code AS kind, l.text,
                        l.agent_id, ls.code AS stage, l.source_turn_id, l.created_at, l.expires_at, l.topic
                   FROM memory.lessons l
                   LEFT JOIN projects.projects p ON p.id = l.project_id
                   JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE p.project_path = '/mnt/data/projects/holdfast'
                    AND ls.code != 'archived'
                    AND (l.expires_at IS NULL OR l.expires_at > NOW())
                    AND NOT EXISTS (
                        SELECT 1
                          FROM memory.lesson_relations r
                          JOIN memory.lesson_relation_kinds rk ON rk.id = r.kind_id AND rk.code = 'supersedes'
                          JOIN memory.lesson_relation_statuses rs ON rs.id = r.status_id AND rs.code = 'confirmed'
                         WHERE r.to_lesson_id = l.id
                    )
                  ORDER BY l.created_at DESC