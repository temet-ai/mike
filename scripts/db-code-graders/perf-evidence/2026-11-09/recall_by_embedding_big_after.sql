WITH nearest_chunks AS (
    SELECT c.turn_id,
           c.chunk_embedding <=> (SELECT v FROM public.qvs WHERE label = 'turn')::vector AS distance
    FROM memory.operator_turn_chunks c
    JOIN memory.operator_turns ot ON c.turn_id = ot.id
    JOIN projects.projects p ON p.id = ot.project_id
    WHERE p.project_path = '/mnt/data/projects/temet/temet-proxy'
      AND c.embedding_model = 'nomic-embed-text-v1.5'
      AND (ot.opened_by IS NULL OR ot.opened_by = 'operator')
    ORDER BY c.chunk_embedding <=> (SELECT v FROM public.qvs WHERE label = 'turn')::vector
    LIMIT 200
), chunk_distances AS (
    SELECT turn_id, MIN(distance) AS min_distance
    FROM nearest_chunks
    GROUP BY turn_id
), ranked AS (
    SELECT turn_id, min_distance
    FROM (
        SELECT turn_id, min_distance, MIN(min_distance) OVER () AS best_distance
        FROM chunk_distances
    ) scored
    WHERE min_distance <= LEAST(0.4, best_distance + 0.08)
    ORDER BY min_distance ASC
    LIMIT 10
)
SELECT turn_id, (1.0 - min_distance)::real AS rank FROM ranked ORDER BY min_distance
