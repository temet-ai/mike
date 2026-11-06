SELECT rsp.request_id, rsp.override_applied
  FROM requests.system_prompts rsp
 WHERE rsp.prompt_sha256 = '5eb379b95c35bd3edcd3726325032aac393ad3302ef6b95399574b1ffa3537f5'
