SELECT session_uuid AS "session_uuid!", runtime AS "runtime!", source AS "source!",
       source_version, fidelity AS "fidelity!", completeness AS "completeness!",
       observed_at, lifecycle, lifecycle_at, control, requested_model, observed_model,
       usage_scope, usage_basis, input_tokens, cached_input_tokens, output_tokens,
       reasoning_output_tokens, total_tokens, context_window,
       model_call_count AS "model_call_count!"
FROM runtime.session_observation_summary_v WHERE session_uuid = ANY($1)
