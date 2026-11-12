DELETE FROM runtime.codex_projection_generations WHERE generation_id IN
     (SELECT g.generation_id FROM runtime.codex_projection_generations g
      WHERE g.state IN ('retired', 'failed') AND g.created_at < $1::timestamptz - interval '1 day'
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_heads h WHERE h.active_generation_id = g.generation_id)
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_applications a WHERE a.generation_id = g.generation_id)
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_effects e WHERE e.generation_id = g.generation_id)
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_item_state i WHERE i.generation_id = g.generation_id) LIMIT $2)
