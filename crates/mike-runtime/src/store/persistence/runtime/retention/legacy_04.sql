DELETE FROM runtime.codex_observed_requests WHERE id IN
     (SELECT id FROM runtime.codex_observed_requests
      WHERE observed_at < $1::timestamptz - interval '90 days' ORDER BY observed_at LIMIT $2)
