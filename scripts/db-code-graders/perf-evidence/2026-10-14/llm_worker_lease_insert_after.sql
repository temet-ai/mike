INSERT INTO llm.calls
    (role, model, generation_id, prompt_tokens, completion_tokens,
     reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
     http_status, session_id, steerer_id, operator_turn_id, request_body,
     machine_uuid)
VALUES ('session_judge', 'codex/gpt-6-sol', 'gen-1a2b3c', 5200, 310, 120, 4096, 0.0042, 5100, 'ok',
        200, 1779708, NULL, NULL,
        '{"model":"codex/gpt-6-sol","messages":[]}'::jsonb, '3f6c7a52-91d4-4c1e-b7a0-5e2d8c4f1a90'::uuid)
RETURNING id
