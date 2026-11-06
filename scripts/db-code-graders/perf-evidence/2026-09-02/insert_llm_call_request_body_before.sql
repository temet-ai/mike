INSERT INTO llm.calls
       (role, model, generation_id, prompt_tokens, completion_tokens,
        reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
        http_status, session_id, steerer_id, operator_turn_id)
VALUES ('session_judge', 'openai/gpt-4o-mini', 'gen-perf-before', 500, 120,
        0, 0, 0.01, 900, 'ok',
        200, NULL, NULL, NULL)
RETURNING id;
