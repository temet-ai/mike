SELECT l.id, l.text FROM memory.lessons l
             JOIN memory.lesson_stages stage ON stage.id = l.stage_id
             WHERE l.id > 0 AND l.archived_at IS NULL
               AND (l.embedding IS NULL OR l.embedding_model IS DISTINCT FROM 'nomic-embed-text-v1.5')
               AND (l.expires_at IS NULL OR l.expires_at > NOW())
               AND stage.code IN ('daily', 'long_term')
               AND NOT EXISTS (
                   SELECT 1 FROM memory.lesson_relations r
                   JOIN memory.lesson_relation_kinds k ON k.id = r.kind_id AND k.code = 'supersedes'
                   JOIN memory.lesson_relation_statuses s ON s.id = r.status_id AND s.code = 'confirmed'
                   WHERE r.to_lesson_id = l.id
               )
             ORDER BY l.id LIMIT 16;
