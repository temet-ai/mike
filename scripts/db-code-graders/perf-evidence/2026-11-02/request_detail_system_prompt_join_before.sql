SELECT rsp.request_id, sp.blocks
  FROM requests.system_prompts rsp
  LEFT JOIN system_prompts.system_prompts sp ON sp.prompt_sha256 = rsp.prompt_sha256
 WHERE rsp.request_id = 733799
