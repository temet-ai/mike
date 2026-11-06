SELECT DISTINCT p.project_path
                   FROM memory.lessons l
                   JOIN projects.projects p ON p.id = l.project_id
                   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                   JOIN memory.lesson_sources src ON src.id = l.source_id
                  WHERE (ls.code IN ('daily', 'long_term')
                         OR (ls.code = 'raw' AND src.code = 'operator'))
                    AND (l.expires_at IS NULL OR l.expires_at > NOW())
                    AND l.project_id IS NOT NULL