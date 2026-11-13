INSERT INTO runtime.model_calls AS c
    (session_id, call_id, observed_at, source, source_version, fidelity, completeness,
     requested_model, observed_model, status, duration_ms, http_status, usage_scope, usage_basis,
     input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
     context_window)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
ON CONFLICT (session_id, call_id) DO UPDATE SET
    observed_at = EXCLUDED.observed_at,
    source = EXCLUDED.source,
    source_version = EXCLUDED.source_version,
    fidelity = EXCLUDED.fidelity,
    completeness = EXCLUDED.completeness,
    requested_model = COALESCE(EXCLUDED.requested_model, c.requested_model),
    observed_model = COALESCE(EXCLUDED.observed_model, c.observed_model),
    status = COALESCE(EXCLUDED.status, c.status),
    duration_ms = COALESCE(EXCLUDED.duration_ms, c.duration_ms),
    http_status = COALESCE(EXCLUDED.http_status, c.http_status),
    usage_scope = COALESCE(EXCLUDED.usage_scope, c.usage_scope),
    usage_basis = COALESCE(EXCLUDED.usage_basis, c.usage_basis),
    input_tokens = COALESCE(EXCLUDED.input_tokens, c.input_tokens),
    cached_input_tokens = COALESCE(EXCLUDED.cached_input_tokens, c.cached_input_tokens),
    output_tokens = COALESCE(EXCLUDED.output_tokens, c.output_tokens),
    reasoning_output_tokens = COALESCE(EXCLUDED.reasoning_output_tokens, c.reasoning_output_tokens),
    total_tokens = COALESCE(EXCLUDED.total_tokens, c.total_tokens),
    context_window = COALESCE(EXCLUDED.context_window, c.context_window)
WHERE EXCLUDED.observed_at >= c.observed_at
