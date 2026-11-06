SELECT
  (SELECT count(*) FROM (
      SELECT 1 FROM memory.knowledge_embedding_state e
       WHERE e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28' AND e.completed_at IS NOT NULL
       LIMIT 1001) ready) AS ready,
  (SELECT count(*) FROM (
      SELECT 1 FROM memory.knowledge_embedding_state e
      CROSS JOIN LATERAL (
          SELECT 1 FROM memory.knowledge_embedding_sources_v s
           WHERE s.source_kind = e.source_kind AND s.source_id = e.source_id
             AND s.source_updated_at IS NOT DISTINCT FROM e.source_updated_at
             AND s.source_seq IS NOT DISTINCT FROM e.source_seq
             AND s.source_hash IS NOT DISTINCT FROM e.source_hash
           LIMIT 1) current_source
       WHERE e.model_key = '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28' AND e.last_error IS NOT NULL
       LIMIT 1001) failed) AS failed;
