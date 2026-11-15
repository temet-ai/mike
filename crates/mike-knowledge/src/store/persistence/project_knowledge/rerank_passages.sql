WITH wanted AS (
    SELECT DISTINCT r.reference,
           split_part(r.reference, ':', 1) AS kind,
           substr(r.reference, position(':' IN r.reference) + 1) AS key
      FROM unnest($1::text[]) AS r(reference)
)
(SELECT DISTINCT ON (w.reference) w.reference AS "reference!", c.body AS "body!"
   FROM wanted w
   JOIN memory.knowledge_embedding_state e
     ON e.reference = w.reference
    AND e.completed_at IS NOT NULL AND ($3::text[] IS NULL OR e.model_key = ANY($3::text[]))
   JOIN memory.knowledge_embedding_chunks c ON c.state_id = e.id
  ORDER BY w.reference, c.embedding <=> $2::text::public.vector NULLS LAST, c.ordinal)
UNION ALL
(SELECT DISTINCT ON (w.reference) w.reference, c.chunk_text
   FROM wanted w
   JOIN memory.operator_turn_chunks c
     ON c.turn_id = CASE WHEN w.key ~ '^[0-9]{1,18}$' THEN w.key::bigint END
    AND ($4::text IS NULL OR c.embedding_model = $4::text)
  WHERE w.kind = 'turn'
  ORDER BY w.reference, c.chunk_embedding <=> $2::text::public.vector NULLS LAST, c.chunk_idx)
