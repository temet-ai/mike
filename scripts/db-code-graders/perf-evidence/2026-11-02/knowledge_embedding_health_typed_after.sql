SELECT count(*),
       count(*) FILTER (WHERE e.completed_at IS NOT NULL AND e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28'
                          AND e.source_updated_at IS NOT DISTINCT FROM s.source_updated_at
                          AND e.source_seq IS NOT DISTINCT FROM s.source_seq
                          AND e.source_hash IS NOT DISTINCT FROM s.source_hash),
       count(*) FILTER (WHERE e.last_error IS NOT NULL AND e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28'
                          AND e.source_updated_at IS NOT DISTINCT FROM s.source_updated_at
                          AND e.source_seq IS NOT DISTINCT FROM s.source_seq
                          AND e.source_hash IS NOT DISTINCT FROM s.source_hash)
  FROM memory.knowledge_embedding_sources_v s
  LEFT JOIN memory.knowledge_embedding_state e
    ON e.source_kind = s.source_kind AND e.source_id = s.source_id
 WHERE (993478::bigint IS NULL OR s.project_id = 993478)
