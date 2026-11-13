DELETE FROM runtime.codex_observed_threads WHERE (namespace_id, thread_id) IN
     (SELECT namespace_id, thread_id FROM runtime.codex_observed_threads
      WHERE last_seen_at < $1::timestamptz - interval '90 days' ORDER BY last_seen_at LIMIT $2)
