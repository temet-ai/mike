DELETE FROM runtime.codex_item_state WHERE (generation_id, thread_id, turn_id, item_id) IN
     (SELECT i.generation_id, i.thread_id, i.turn_id, i.item_id FROM runtime.codex_item_state i
      JOIN runtime.codex_projection_generations g ON g.generation_id = i.generation_id
      WHERE g.state IN ('retired', 'failed') AND g.created_at < $1::timestamptz - interval '1 day' LIMIT $2)
