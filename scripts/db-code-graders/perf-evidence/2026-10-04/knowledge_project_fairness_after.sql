WITH pending AS (
          SELECT s.reference, s.content_hash, s.body, s.project_id,
            row_number() OVER (PARTITION BY s.project_id ORDER BY e.next_attempt_at NULLS FIRST, s.reference) AS project_rank
          FROM memory.knowledge_embedding_sources_v s
          LEFT JOIN memory.knowledge_embedding_state e ON e.reference = s.reference
          WHERE e.reference IS NULL OR e.content_hash <> s.content_hash OR e.model_key <> '4f945851e69962957ac4bad33b67290b513119e102e220862406632a8a43c3b1'
             OR (e.completed_at IS NULL AND e.next_attempt_at <= now() AND (e.attempts < 10 OR e.last_error LIKE '[transient]%'))
          ) SELECT reference, content_hash, body FROM pending
          ORDER BY project_rank, project_id, reference LIMIT 16;