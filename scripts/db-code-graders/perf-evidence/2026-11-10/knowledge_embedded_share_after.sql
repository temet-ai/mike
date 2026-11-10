SELECT (SELECT count(*)
          FROM (SELECT 1
                  FROM memory.knowledge_embedding_state e
                  JOIN memory.knowledge_embedding_sources_v src
                    ON src.source_kind = e.source_kind AND src.source_id = e.source_id
                 WHERE src.project_id = 993478
                   AND e.source_updated_at IS NOT DISTINCT FROM src.source_updated_at
                   AND e.source_seq IS NOT DISTINCT FROM src.source_seq
                   AND e.source_hash IS NOT DISTINCT FROM src.source_hash
                   AND e.model_key = '0fd0b5032acd9cef65179bc5fe0880ef2177755f4a18f97546a7299e85771b61' AND e.completed_at IS NOT NULL
                 LIMIT (SELECT GREATEST(ceil(reltuples::float8 / 5), 0)::bigint
                          FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass)) lim) * 5
       >= (SELECT reltuples FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass) AS "enabled!"
