DELETE FROM runtime.codex_observed_turns WHERE (namespace_id, turn_id) IN
     (SELECT namespace_id, turn_id FROM runtime.codex_observed_turns
      WHERE COALESCE(stopped_at, first_seen_at) < $1::timestamptz - interval '90 days' ORDER BY first_seen_at LIMIT $2)
