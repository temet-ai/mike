INSERT INTO llm.calls
       (role, model, generation_id, prompt_tokens, completion_tokens,
        reasoning_tokens, cached_tokens, cost_usd, latency_ms, outcome,
        http_status, session_id, steerer_id, operator_turn_id, request_body)
VALUES ('session_judge', 'openai/gpt-4o-mini', 'gen-perf-after', 500, 120,
        0, 0, 0.01, 900, 'ok',
        200, NULL, NULL, NULL,
        '{"model":"openai/gpt-4o-mini","messages":[{"role":"system","content":"You are a session judge."},{"role":"user","content":"RELATED SESSIONS\n\n- Fix the mailbox stranding bug: mailbox delivery must ack before the sender clears"}],"response_format":{"type":"json_schema","json_schema":{"name":"session_verdict","strict":true,"schema":{}}},"provider":{"require_parameters":true}}'::jsonb)
RETURNING id;
