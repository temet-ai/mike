SELECT s.reference, s.content_hash, s.body
          FROM memory.knowledge_embedding_sources_v s
          LEFT JOIN memory.knowledge_embedding_state e ON e.reference = s.reference
          WHERE e.reference IS NULL OR e.content_hash <> s.content_hash OR e.model_key <> 'knowledge-performance-model'
             OR (e.completed_at IS NULL AND e.next_attempt_at <= now() AND e.attempts < 10)
          ORDER BY e.next_attempt_at NULLS FIRST, s.reference LIMIT 16;
