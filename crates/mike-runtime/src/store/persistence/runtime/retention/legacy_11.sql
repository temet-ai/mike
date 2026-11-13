DELETE FROM runtime.events WHERE id IN
     (SELECT e.id FROM runtime.events e WHERE e.created_at < $1::timestamptz - interval '90 days'
        AND NOT EXISTS (SELECT 1 FROM runtime.codex_receipt_context c WHERE c.event_id = e.id)
      ORDER BY e.id LIMIT $2)
