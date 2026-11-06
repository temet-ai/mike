UPDATE sessions.sessions
   SET flow_capture_version = 1,
       flow_admitted_at = clock_timestamp(),
       flow_producer = 'claude_code',
       flow_hook_event_name = 'SessionStart',
       flow_start_source = 'startup'
 WHERE id = 1106460 AND flow_capture_version IS NULL
RETURNING id, flow_capture_version, flow_admitted_at, flow_producer,
          flow_hook_event_name, flow_start_source;
