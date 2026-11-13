DELETE FROM runtime.codex_observed_tools WHERE (namespace_id, call_id) IN
     (SELECT namespace_id, call_id FROM runtime.codex_observed_tools
      WHERE COALESCE(completed_at, started_at, '-infinity'::timestamptz) < $1::timestamptz - interval '90 days'
      ORDER BY started_at NULLS FIRST LIMIT $2)
