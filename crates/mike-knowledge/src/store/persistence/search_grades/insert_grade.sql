INSERT INTO memory.search_grades
       (event_id, candidates, used_references, plain_positions, reranked_positions,
        first_ten, turn_prompt, relevant_references)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (event_id) DO NOTHING
