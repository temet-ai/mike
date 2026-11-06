WITH pending AS (
          SELECT s.reference, s.content_hash, s.body, s.project_id,
            row_number() OVER (PARTITION BY s.project_id ORDER BY e.next_attempt_at NULLS FIRST, s.reference) AS project_rank
          FROM perf_before.knowledge_embedding_sources_v s
          LEFT JOIN perf_before.knowledge_embedding_state e ON e.reference = s.reference
          WHERE e.reference IS NULL OR e.content_hash <> s.content_hash OR e.model_key <> '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28'
             OR (e.completed_at IS NULL AND e.next_attempt_at <= now() AND (e.attempts < 10 OR e.last_error LIKE '[transient]%'))
          ) SELECT reference, content_hash, body FROM pending
          ORDER BY project_rank, project_id, reference LIMIT 16
