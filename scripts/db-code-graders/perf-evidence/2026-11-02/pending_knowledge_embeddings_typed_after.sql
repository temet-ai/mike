WITH candidates AS (
    SELECT s.source_kind, s.source_id, s.reference, s.project_id,
           s.source_updated_at, s.source_seq, s.source_hash,
           row_number() OVER (PARTITION BY s.project_id
                              ORDER BY e.next_attempt_at NULLS FIRST, s.source_kind, s.source_id) AS project_rank
      FROM memory.knowledge_embedding_sources_v s
      LEFT JOIN memory.knowledge_embedding_state e
        ON e.source_kind = s.source_kind AND e.source_id = s.source_id
     WHERE e.id IS NULL OR e.model_key <> '44504dc8418beb54a3300bc02c99c0ac2c667ac837d1c8e6234bf25241084f28'
        OR e.source_updated_at IS DISTINCT FROM s.source_updated_at
        OR e.source_seq IS DISTINCT FROM s.source_seq
        OR e.source_hash IS DISTINCT FROM s.source_hash
        OR (e.completed_at IS NULL AND e.next_attempt_at <= now()
            AND (e.attempts < 10 OR e.last_error LIKE '[transient]%'))
), picked AS MATERIALIZED (
    SELECT source_kind, source_id, reference, project_id, source_updated_at, source_seq, source_hash, project_rank
      FROM candidates
     ORDER BY project_rank, project_id, source_kind, source_id
     LIMIT 16
), bodies AS (
    SELECT 'artifact'::text AS source_kind, r.id AS source_id, r.title || E'\n\n' || r.body AS body
      FROM memory.knowledge_revisions r
     WHERE r.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'artifact'))
    UNION ALL
    SELECT 'lesson', l.id, l.text
      FROM memory.lessons l
     WHERE l.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'lesson'))
    UNION ALL
    SELECT 'note', n.id, n.title || E'\n\n' || n.body
      FROM workspace.session_notes n
     WHERE n.id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'note'))
    UNION ALL
    SELECT 'thread', tx.thread_id, tx.body
      FROM mailbox.thread_text_v tx
     WHERE tx.thread_id = ANY(ARRAY(SELECT source_id FROM picked WHERE source_kind = 'thread'))
)
SELECT p.source_kind, p.source_id, p.reference, p.source_updated_at, p.source_seq, p.source_hash, b.body
  FROM picked p
  JOIN bodies b ON b.source_kind = p.source_kind AND b.source_id = p.source_id
 ORDER BY p.project_rank, p.project_id, p.source_kind, p.source_id
