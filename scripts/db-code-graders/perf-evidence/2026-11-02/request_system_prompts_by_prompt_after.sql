SELECT rsp.request_id, rsp.override_applied
  FROM requests.system_prompts rsp
 WHERE rsp.system_prompt_id IN (
           SELECT sp.id FROM system_prompts.system_prompts sp WHERE sp.prompt_sha256 = '5eb379b95c35bd3edcd3726325032aac393ad3302ef6b95399574b1ffa3537f5'
       )
