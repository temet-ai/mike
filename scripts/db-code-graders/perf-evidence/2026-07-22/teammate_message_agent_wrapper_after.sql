UPDATE prompt_kinds.classifications pkc
   SET kind_id = 3
  FROM memory.operator_turns ot
 WHERE pkc.request_id = ot.start_request_id
   AND pkc.kind_id = 4
   AND ot.user_prompt LIKE '%<agent-message from=%'
