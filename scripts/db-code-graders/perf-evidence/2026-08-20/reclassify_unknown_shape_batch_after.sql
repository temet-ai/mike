SELECT rsp.request_id, r.request_body
  FROM requests.system_prompts rsp
  JOIN requests.requests r ON r.id = rsp.request_id
 WHERE rsp.request_id > 0
   AND rsp.prompt_sha256 IS NULL
   AND rsp.system_prompt_id IS NULL
   AND rsp.unknown_blocks->>'reason' IN ('non_3_block', 'string_form')
   AND r.request_body IS NOT NULL
 ORDER BY rsp.request_id ASC
 LIMIT 200
