DELETE FROM runtime.codex_receipt_context WHERE event_id IN
     (SELECT c.event_id FROM runtime.codex_receipt_context c
      WHERE c.created_at < $1::timestamptz - interval '90 days'
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_applications a WHERE a.event_id = c.event_id)
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_effects e WHERE e.event_id = c.event_id) LIMIT $2)
