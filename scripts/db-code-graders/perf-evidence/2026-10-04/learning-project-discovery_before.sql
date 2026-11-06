SELECT DISTINCT p.project_path
                   FROM memory.lessons l
                   JOIN projects.projects p ON p.id = l.project_id
                   JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                  WHERE ls.code IN ('daily', 'long_term')
                    AND (l.expires_at IS NULL OR l.expires_at > NOW())
                    AND l.project_id IS NOT NULL