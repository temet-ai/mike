SELECT DISTINCT l.project_id
                   FROM memory.lessons l
                   JOIN memory.lesson_stages ls ON ls.id = l.stage_id AND ls.code = 'raw'
                  WHERE l.embedded_at IS NULL
                    AND l.project_id IS NOT NULL
                    AND l.source_id NOT IN (SELECT id FROM memory.lesson_sources WHERE code = 'carry')
                    AND NOT EXISTS (
                        SELECT 1 FROM memory.lesson_shape_findings f
                         WHERE f.lesson_id = l.id AND f.resolution IS NULL
                    )
                    AND l.created_at < now() - interval '60 seconds'
