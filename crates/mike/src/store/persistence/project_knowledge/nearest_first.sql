SELECT (SELECT count(*)
          FROM (SELECT 1
                  FROM memory.knowledge_embedding_state e
                  JOIN memory.knowledge_sources_with_sessions_v src
                    ON src.source_kind = e.source_kind AND src.source_id = e.source_id
                 WHERE src.project_id = $1
                   AND e.source_updated_at IS NOT DISTINCT FROM src.source_updated_at
                   AND e.source_seq IS NOT DISTINCT FROM src.source_seq
                   AND e.source_hash IS NOT DISTINCT FROM src.source_hash
                   AND e.model_key = ANY($2::text[]) AND e.completed_at IS NOT NULL
                 LIMIT (SELECT GREATEST(ceil(reltuples::float8 / 5), 0)::bigint
                          FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass)) lim) * 5
       >= (SELECT reltuples FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass) AS "enabled!"
