DELETE FROM runtime.codex_observed_hook_runs WHERE (namespace_id, trace_id, span_id) IN
     (SELECT namespace_id, trace_id, span_id FROM runtime.codex_observed_hook_runs
      WHERE started_at < $1::timestamptz - interval '90 days' ORDER BY started_at LIMIT $2)
