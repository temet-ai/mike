SELECT count(*) AS "total!",
       count(*) FILTER (WHERE e.completed_at IS NOT NULL AND e.model_key = ANY($2::text[])
           AND e.source_updated_at IS NOT DISTINCT FROM s.source_updated_at
           AND e.source_seq IS NOT DISTINCT FROM s.source_seq
           AND e.source_hash IS NOT DISTINCT FROM s.source_hash) AS "ready!",
       count(*) FILTER (WHERE e.last_error IS NOT NULL AND e.model_key = ANY($2::text[])
           AND e.source_updated_at IS NOT DISTINCT FROM s.source_updated_at
           AND e.source_seq IS NOT DISTINCT FROM s.source_seq
           AND e.source_hash IS NOT DISTINCT FROM s.source_hash) AS "failed!",
       count(*) FILTER (WHERE e.completed_at IS NOT NULL AND e.model_key = $3
           AND e.source_updated_at IS NOT DISTINCT FROM s.source_updated_at
           AND e.source_seq IS NOT DISTINCT FROM s.source_seq
           AND e.source_hash IS NOT DISTINCT FROM s.source_hash) AS "current_version!"
  FROM memory.knowledge_sources_with_sessions_v s
  LEFT JOIN memory.knowledge_embedding_state e
    ON e.source_kind = s.source_kind AND e.source_id = s.source_id
 WHERE s.project_id = $1
