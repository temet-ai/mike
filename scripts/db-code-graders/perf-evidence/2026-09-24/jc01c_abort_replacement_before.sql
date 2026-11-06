INSERT INTO memory.operator_turns (
    session_id, project_id,
    start_request_id, end_request_id,
    user_prompt
 )
 SELECT r.session_id, r.project_id, r.id, r.id, r.human_prompt
   FROM requests.requests r
   JOIN requests.turn_types tt ON tt.id = r.turn_type_id
   JOIN requests.turn_origins ro ON ro.id = r.turn_origin_id
  WHERE r.id = 234687
    AND tt.code = 'user_prompt'
    AND ro.code IN ('primary', 'primary_first')
    AND NOT EXISTS (
        SELECT 1 FROM memory.operator_turns
        WHERE session_id = r.session_id AND closed_at IS NULL
    )
    AND r.human_prompt <> ''
    AND r.human_prompt NOT LIKE '%Describe your most recent action in 3-5 words%'
    AND r.human_prompt NOT LIKE '%Stop hook feedback:%'
    AND r.human_prompt NOT LIKE '%<teammate-message%'
    AND r.human_prompt NOT LIKE '%<agent-message from=%'
    AND r.human_prompt NOT LIKE '%The user stepped away and is coming back%'
    AND r.human_prompt NOT LIKE '%[SYSTEM NOTIFICATION - NOT USER INPUT]%'
    AND r.human_prompt NOT LIKE '%This session is being continued from a previous conversation%'
    AND r.human_prompt NOT LIKE '%Summarize this portion of a Claude Code session transcript%'
    AND r.human_prompt NOT LIKE '%Analyze this Claude Code session and extract structured facets%'
    AND r.human_prompt NOT LIKE '%has the following stopping condition been satisfied%'
 RETURNING id
