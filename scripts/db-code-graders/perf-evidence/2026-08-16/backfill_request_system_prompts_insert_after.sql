INSERT INTO system_prompts.system_prompts
    (prompt_sha256, blocks, first_seen_at, last_seen_at)
 SELECT 'bcb6453e147d7a888413fd6caaab2e05fcb87142ed79701b8e6401c4ff4b0f77', '[{"type":"text","text":"billing-header"},{"type":"text","text":"identity preamble","cache_control":{"type":"ephemeral"}},{"type":"text","text":"perf-check probe body","cache_control":{"type":"ephemeral","ttl":"1h"}}]'::jsonb, r.created_at, r.created_at
   FROM requests.requests r
  WHERE r.id = 52052
    AND NOT EXISTS (
        SELECT 1 FROM system_prompts.system_prompts
         WHERE prompt_sha256 = 'bcb6453e147d7a888413fd6caaab2e05fcb87142ed79701b8e6401c4ff4b0f77'
    )
