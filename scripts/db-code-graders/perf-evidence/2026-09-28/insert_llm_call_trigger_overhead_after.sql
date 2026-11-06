INSERT INTO llm.calls
    (role, model, generation_id, prompt_tokens, completion_tokens,
     reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
     http_status, session_id, steerer_id, operator_turn_id, request_body)
VALUES ('steerer', 'openai/gpt-5.6-luna', 'gen-perf-check', 1200, 300,
        0, 0, 0.015, 900, 'ok',
        200, NULL, 14, NULL, NULL)
