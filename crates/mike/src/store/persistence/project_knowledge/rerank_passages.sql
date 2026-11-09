WITH wanted AS (
    SELECT DISTINCT r.reference,
           split_part(r.reference, ':', 1) AS kind,
           substr(r.reference, position(':' IN r.reference) + 1) AS key
      FROM unnest($1::text[]) AS r(reference)
), sources AS (
    SELECT w.reference, 'artifact'::text AS source_kind,
           CASE WHEN w.key ~ '^[0-9]{1,18}$' THEN w.key::bigint END AS source_id
      FROM wanted w
     WHERE w.kind = 'artifact'
    UNION ALL
    SELECT w.reference, 'lesson', l.id
      FROM wanted w
      JOIN memory.lessons l
        ON l.uuid = CASE WHEN w.key ~ '^[0-9a-fA-F-]{36}$' THEN w.key::uuid END
     WHERE w.kind = 'lesson'
    UNION ALL
    SELECT w.reference, 'note', n.id
      FROM wanted w
      JOIN workspace.session_notes n
        ON n.note_uuid = CASE WHEN w.key ~ '^[0-9a-fA-F-]{36}$' THEN w.key::uuid END
     WHERE w.kind = 'note'
    UNION ALL
    SELECT w.reference, 'thread', t.id
      FROM wanted w
      JOIN mailbox.threads t
        ON t.thread_uuid = CASE WHEN w.key ~ '^[0-9a-fA-F-]{36}$' THEN w.key::uuid END
     WHERE w.kind = 'thread'
)
SELECT DISTINCT ON (s.reference) s.reference AS "reference!", c.body AS "body!"
  FROM sources s
  JOIN memory.knowledge_embedding_state e
    ON e.source_kind = s.source_kind AND e.source_id = s.source_id
   AND e.completed_at IS NOT NULL AND ($3::text IS NULL OR e.model_key = $3::text)
  JOIN memory.knowledge_embedding_chunks c ON c.state_id = e.id
 ORDER BY s.reference, c.embedding <=> $2::text::public.vector NULLS LAST, c.ordinal
