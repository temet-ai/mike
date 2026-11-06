SELECT count(*),
          count(*) FILTER (WHERE e.completed_at IS NOT NULL AND e.content_hash = s.content_hash AND e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28'),
          count(*) FILTER (WHERE e.last_error IS NOT NULL AND e.content_hash = s.content_hash AND e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28')
          FROM perf_before.knowledge_embedding_sources_v s
          LEFT JOIN perf_before.knowledge_embedding_state e ON e.reference = s.reference WHERE (993478::bigint IS NULL OR s.project_id = 993478)
