SELECT (SELECT count(*)
          FROM (SELECT 1
                  FROM memory.knowledge_embedding_state e
                 WHERE e.project_id = $1 AND e.model_key = ANY($2::text[]) AND e.completed_at IS NOT NULL
                 LIMIT (SELECT GREATEST(ceil(reltuples::float8 / 5), 0)::bigint
                          FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass)) lim) * 5
       >= (SELECT reltuples FROM pg_class WHERE oid = 'memory.knowledge_embedding_state'::regclass) AS "enabled!"
