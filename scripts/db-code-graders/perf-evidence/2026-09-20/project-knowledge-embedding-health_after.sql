SELECT count(*),
          count(*) FILTER (WHERE e.completed_at IS NOT NULL AND e.content_hash = s.content_hash AND e.model_key = 'knowledge-performance-model'),
          count(*) FILTER (WHERE e.last_error IS NOT NULL AND e.content_hash = s.content_hash AND e.model_key = 'knowledge-performance-model')
          FROM memory.knowledge_embedding_sources_v s
          LEFT JOIN memory.knowledge_embedding_state e ON e.reference = s.reference WHERE (NULL::bigint IS NULL OR s.project_id = NULL);
