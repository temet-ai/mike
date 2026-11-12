DELETE FROM runtime.codex_projection_applications WHERE (generation_id, event_id) IN
     (SELECT a.generation_id, a.event_id FROM runtime.codex_projection_applications a
      JOIN runtime.codex_projection_generations g ON g.generation_id = a.generation_id
      WHERE g.state IN ('retired', 'failed') AND g.created_at < $1::timestamptz - interval '1 day'
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_effects e
                        WHERE e.generation_id = a.generation_id AND e.event_id = a.event_id) LIMIT $2)
