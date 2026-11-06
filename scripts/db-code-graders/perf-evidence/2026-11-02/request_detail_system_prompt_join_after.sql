SELECT rsp.request_id, sp.blocks
  FROM requests.system_prompts rsp
  LEFT JOIN system_prompts.system_prompts sp ON sp.id = rsp.system_prompt_id
 WHERE rsp.request_id = 733799
