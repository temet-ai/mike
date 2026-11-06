INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id,
    end_request_id,
    user_prompt
 )
 SELECT 652849, 378477, 234687, 234687, 'please refactor the store'
 WHERE NOT EXISTS (
     SELECT 1 FROM memory.operator_turns
     WHERE session_id = 652849 AND closed_at IS NULL
 )
 AND 'please refactor the store' <> ''
 AND 'please refactor the store' NOT LIKE '%Describe your most recent action in 3-5 words%'
 AND 'please refactor the store' NOT LIKE '%Stop hook feedback:%'
 AND 'please refactor the store' NOT LIKE '%<teammate-message%'
 AND 'please refactor the store' NOT LIKE '%<agent-message from=%'
 AND 'please refactor the store' NOT LIKE '%The user stepped away and is coming back%'
 AND 'please refactor the store' NOT LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
 AND 'please refactor the store' NOT LIKE '%This session is being continued from a previous conversation%'
 AND 'please refactor the store' NOT LIKE '%Summarize this portion of a Claude Code session transcript%'
 AND 'please refactor the store' NOT LIKE '%Analyze this Claude Code session and extract structured facets%'
 AND 'please refactor the store' NOT LIKE '%has the following stopping condition been satisfied%'
 RETURNING id
