DELETE FROM memory.operator_turns ot
 WHERE ot.user_prompt IS NULL
    OR ot.user_prompt = ''
    OR ot.user_prompt LIKE '%Describe your most recent action in 3-5 words%'
    OR ot.user_prompt LIKE '%Stop hook feedback:%'
    OR ot.user_prompt LIKE '%<teammate-message%'
    OR ot.user_prompt LIKE '%<agent-message from=%'
    OR ot.user_prompt LIKE '%The user stepped away and is coming back%'
    OR ot.user_prompt LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
    OR ot.user_prompt LIKE '%This session is being continued from a previous conversation%'
    OR ot.user_prompt LIKE '%Summarize this portion of a Claude Code session transcript%'
    OR ot.user_prompt LIKE '%Analyze this Claude Code session and extract structured facets%'
    OR ot.user_prompt LIKE '%has the following stopping condition been satisfied%';
