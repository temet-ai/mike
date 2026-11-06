INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id,
    end_request_id,
    user_prompt
 )
 SELECT 719112, 313870, 262413, 262413, 'Fix the flaky retry test'
 WHERE NOT EXISTS (
     SELECT 1 FROM memory.operator_turns
     WHERE session_id = 719112 AND closed_at IS NULL
 )
 AND 'Fix the flaky retry test' <> ''
 AND 'Fix the flaky retry test' NOT LIKE '%Describe your most recent action in 3-5 words%'
 AND 'Fix the flaky retry test' NOT LIKE '%Stop hook feedback:%'
 AND 'Fix the flaky retry test' NOT LIKE '%<teammate-message%'
 AND 'Fix the flaky retry test' NOT LIKE '%<agent-message from=%'
 AND 'Fix the flaky retry test' NOT LIKE '%The user stepped away and is coming back%'
 AND 'Fix the flaky retry test' NOT LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
 AND 'Fix the flaky retry test' NOT LIKE '%This session is being continued from a previous conversation%'
 AND 'Fix the flaky retry test' NOT LIKE '%Summarize this portion of a Claude Code session transcript%'
 AND 'Fix the flaky retry test' NOT LIKE '%Analyze this Claude Code session and extract structured facets%'
 AND 'Fix the flaky retry test' NOT LIKE '%has the following stopping condition been satisfied%'
 RETURNING id;
