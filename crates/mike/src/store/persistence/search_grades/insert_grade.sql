INSERT INTO memory.search_grades (event_id, candidates, used_references, plain_positions, reranked_positions)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (event_id) DO NOTHING
