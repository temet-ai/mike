INSERT INTO prompt_kinds.classifications (request_id, session_id, kind_id)
SELECT 446648, 1173418,
       CASE
         WHEN 'hello there' LIKE '%Describe your most recent action in 3-5 words%' THEN 1
         WHEN 'hello there' LIKE '%Stop hook feedback:%' THEN 2
         WHEN 'hello there' LIKE '%<teammate-message%' OR 'hello there' LIKE '%<agent-message from=%' THEN 3
         WHEN 'hello there' LIKE '%The user stepped away and is coming back%' THEN 5
         WHEN 'hello there' LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%' THEN 6
         WHEN 'hello there' LIKE '%This session is being continued from a previous conversation%' THEN 7
         WHEN 'hello there' LIKE '%Summarize this portion of a Claude Code session transcript%' THEN 7
         WHEN 'hello there' LIKE '%Analyze this Claude Code session and extract structured facets%' THEN 8
         WHEN 'hello there' LIKE '%has the following stopping condition been satisfied%' THEN 8
         ELSE 4
       END
ON CONFLICT (request_id) DO NOTHING
