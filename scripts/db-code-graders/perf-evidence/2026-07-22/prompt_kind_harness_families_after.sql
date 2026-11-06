UPDATE prompt_kinds.classifications pkc
   SET kind_id = CASE
                   WHEN ot.user_prompt LIKE '%The user stepped away and is coming back%' THEN 5
                   WHEN ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%' THEN 6
                   WHEN ot.user_prompt LIKE '%This session is being continued from a previous conversation%' THEN 7
                   WHEN ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%' THEN 7
                   WHEN ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%' THEN 8
                   WHEN ot.user_prompt LIKE '%has the following stopping condition been satisfied%' THEN 8
                 END
  FROM memory.operator_turns ot
 WHERE pkc.request_id = ot.start_request_id
   AND pkc.kind_id = 4
   AND (
        ot.user_prompt LIKE '%The user stepped away and is coming back%'
     OR ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
     OR ot.user_prompt LIKE '%This session is being continued from a previous conversation%'
     OR ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%'
     OR ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%'
     OR ot.user_prompt LIKE '%has the following stopping condition been satisfied%'
   )
