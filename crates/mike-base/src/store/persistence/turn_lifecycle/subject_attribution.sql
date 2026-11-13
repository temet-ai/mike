UPDATE memory.operator_turns ot
   SET subject_command = NULLIF(substring(ot.user_prompt FROM '^\s*(/[A-Za-z0-9][A-Za-z0-9_:-]*)'), ''),
       subject_agent   = COALESCE(r.subagent_persona, 'main')
  FROM requests.requests r
 WHERE ot.id = $1 AND r.id = ot.start_request_id
