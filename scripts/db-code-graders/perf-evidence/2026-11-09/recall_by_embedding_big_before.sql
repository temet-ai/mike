WITH chunk_distances AS (
    SELECT c.turn_id,
           MIN(c.chunk_embedding <=> (SELECT v FROM public.qvs WHERE label = 'turn')) AS min_distance
    FROM memory.operator_turn_chunks c
    JOIN memory.operator_turns ot ON c.turn_id = ot.id
    JOIN projects.projects p ON p.id = ot.project_id
    WHERE p.project_path = '/mnt/data/projects/temet/temet-proxy'
      AND (ot.opened_by IS NULL OR ot.opened_by = 'operator')
    GROUP BY c.turn_id
), ranked AS (
    SELECT turn_id, min_distance FROM chunk_distances ORDER BY min_distance ASC LIMIT 10
)
SELECT turn_id, (1.0 - min_distance)::real AS rank FROM ranked ORDER BY min_distance
