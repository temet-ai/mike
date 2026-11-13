DELETE FROM runtime.codex_projection_effects WHERE (generation_id, event_id, effect_ordinal) IN
     (SELECT e.generation_id, e.event_id, e.effect_ordinal FROM runtime.codex_projection_effects e
      JOIN runtime.codex_projection_generations g ON g.generation_id = e.generation_id
      WHERE g.state IN ('retired', 'failed') AND g.created_at < $1::timestamptz - interval '1 day' LIMIT $2)
