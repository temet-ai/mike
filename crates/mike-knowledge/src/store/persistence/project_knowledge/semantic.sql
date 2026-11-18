WITH own_turn AS (
    SELECT max(t.id) AS id
    FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
    WHERE s.session_uuid = $7::uuid AND (t.opened_by IS NULL OR t.opened_by = 'operator')
), own_refs AS (
    SELECT 'turn:' || o.id AS reference FROM own_turn o WHERE o.id IS NOT NULL
    UNION ALL
    SELECT vk.kind || ':' || v.id
    FROM own_turn o
    JOIN memory.live_turn_verdicts_v v ON v.operator_turn_id = o.id
    CROSS JOIN (VALUES ('unmet_ask'), ('correction')) vk(kind)
), nearest_knowledge_chunks AS MATERIALIZED (
    SELECT c.state_id, c.embedding <=> $2::text::public.vector AS distance
    FROM memory.knowledge_embedding_chunks c
    WHERE $6::bool
    ORDER BY c.embedding <=> $2::text::public.vector
    LIMIT 1000
), semantic_chunks AS (
    (SELECT e.reference, 1 - (c.embedding <=> $2::text::public.vector) AS score
     FROM memory.knowledge_embedding_chunks c
     JOIN memory.knowledge_embedding_state e ON e.id = c.state_id
     WHERE NOT $6::bool
       AND (e.project_id = $1 OR (e.project_id IS NULL AND e.source_kind = 'artifact')) AND e.reference IS NOT NULL
       AND e.model_key = ANY($3::text[]) AND e.completed_at IS NOT NULL
     ORDER BY c.embedding <=> $2::text::public.vector
     LIMIT 200)
    UNION ALL
    (SELECT e.reference, 1 - n.distance AS score
     FROM nearest_knowledge_chunks n
     JOIN memory.knowledge_embedding_state e ON e.id = n.state_id
     WHERE (e.project_id = $1 OR (e.project_id IS NULL AND e.source_kind = 'artifact')) AND e.reference IS NOT NULL
       AND e.model_key = ANY($3::text[]) AND e.completed_at IS NOT NULL
     ORDER BY n.distance
     LIMIT 200)
    UNION ALL
    (SELECT 'turn:' || c.turn_id, 1 - (c.chunk_embedding <=> $2::text::public.vector)
     FROM memory.operator_turn_chunks c JOIN memory.operator_turns t ON t.id = c.turn_id
     WHERE t.project_id = $1 AND c.embedding_model = $4
       AND (t.opened_by IS NULL OR t.opened_by = 'operator')
     ORDER BY c.chunk_embedding <=> $2::text::public.vector
     LIMIT 200)
)
SELECT reference AS "reference!", score AS "score!"
FROM (SELECT reference, max(score) AS score, max(max(score)) OVER () AS best
      FROM semantic_chunks
      WHERE ($8::text IS NULL OR split_part(reference, ':', 1)
            = CASE WHEN $8::text IN ('memory', 'skill') THEN 'artifact' ELSE $8::text END)
        AND NOT EXISTS (SELECT 1 FROM own_refs o WHERE o.reference = semantic_chunks.reference)
      GROUP BY reference) per_reference
WHERE score >= GREATEST(($5::float8[])[1], best - ($5::float8[])[2])
