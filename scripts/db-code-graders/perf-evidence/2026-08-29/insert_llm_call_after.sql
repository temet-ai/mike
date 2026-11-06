INSERT INTO llm.calls
    (role, model, generation_id, prompt_tokens, completion_tokens,
     reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
     http_status, session_id, steerer_id, operator_turn_id)
VALUES ('steerer', 'moonshotai/kimi-k3', 'gen-01hzxk3n8p', 1200, 40,
        0, NULL, 0.0009, 812, 'ok',
        200, NULL, NULL, NULL);
